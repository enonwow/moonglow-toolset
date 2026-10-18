//! Verified, content-addressed NWSync downloads and read-only Live generations.
//! Transport is supplied by the caller (authenticated HTTPS in the gateway link).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use mg_core::sha1::{hex, sha1};
use mg_erf::compressedbuf::{MAGIC_NSYC, decompress_limited};
use mg_resman::{Container, ResError, ResKey};

use crate::{Module, nwsync};

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub manifest_bytes: usize,
    pub resource_bytes: usize,
    pub total_bytes: u64,
    pub entries: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            manifest_bytes: 16 * 1024 * 1024,
            resource_bytes: nwsync::FILE_LIMIT as usize,
            total_bytes: 2 * 1024 * 1024 * 1024,
            entries: 100_000,
        }
    }
}

/// A verified manifest indexes decoded blobs whose filenames are SHA-1 only.
#[derive(Debug, Clone)]
pub struct NwSyncCache {
    root: PathBuf,
    manifest: String,
    entries: BTreeMap<ResKey, ([u8; 20], u32)>,
}

fn digest_name(name: &str) -> Result<(), String> {
    if name.len() != 40 || !name.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        return Err("NWSync digest must be 40 lowercase hexadecimal characters".into());
    }
    Ok(())
}

fn bounded(mut reader: impl Read, limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    reader.by_ref().take(limit as u64 + 1).read_to_end(&mut out).map_err(|e| e.to_string())?;
    if out.len() > limit {
        return Err("NWSync download exceeds byte limit".into());
    }
    Ok(out)
}

fn verified(path: &Path, hash: &[u8; 20], size: u32) -> Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() != u64::from(size) {
        return Err("NWSync blob has wrong size or is not a regular file".into());
    }
    let bytes = bounded(File::open(path).map_err(|e| e.to_string())?, size as usize)?;
    if sha1(&bytes) != *hash {
        return Err("NWSync blob SHA-1 mismatch".into());
    }
    Ok(bytes)
}

fn directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    // Cache roots are private caller-owned directories, not attacker-selected trees.
    if !fs::symlink_metadata(path).map_err(|e| e.to_string())?.is_dir() {
        return Err("NWSync cache directory is not a regular directory".into());
    }
    Ok(())
}

fn atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_file_name(hex(&sha1(&fastrand::u128(..).to_le_bytes())));
    let mut file =
        OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|e| e.to_string())
}

fn fetch_blob(
    blobs: &Path,
    hash: &[u8; 20],
    size: &u32,
    fetch: &mut impl FnMut(&str) -> Result<Box<dyn Read>, String>,
) -> Result<(), String> {
    let name = hex(hash);
    let path = blobs.join(&name);
    if path.exists() {
        verified(&path, hash, *size)?;
        return Ok(());
    }
    let url = format!("data/sha1/{}/{}/{name}", &name[..2], &name[2..4]);
    let packed_limit =
        (*size as usize).checked_add(1024 * 1024).ok_or("NWSync packed size overflow")?;
    let packed = bounded(fetch(&url)?, packed_limit)?;
    let bytes =
        decompress_limited(&packed, MAGIC_NSYC, *size as usize).map_err(|e| e.to_string())?;
    if bytes.len() != *size as usize || sha1(&bytes) != *hash {
        return Err("NWSync blob SHA-1/size mismatch".into());
    }
    atomic(&path, &bytes)?;
    Ok(())
}

impl NwSyncCache {
    /// Fetches exactly this manifest, never `latest`. The reader callback receives
    /// only generated repository-relative paths; network policy belongs to the caller.
    pub fn fetch(
        root: &Path,
        manifest: &str,
        limits: Limits,
        fetch: &mut impl FnMut(&str) -> Result<Box<dyn Read>, String>,
    ) -> Result<Self, String> {
        digest_name(manifest)?;
        directory(root)?;
        let manifests = root.join("manifests");
        let blobs = root.join("blobs");
        directory(&manifests)?;
        directory(&blobs)?;
        let path = manifests.join(manifest);
        let raw = if path.exists() {
            bounded(File::open(&path).map_err(|e| e.to_string())?, limits.manifest_bytes)?
        } else {
            bounded(fetch(&format!("manifests/{manifest}"))?, limits.manifest_bytes)?
        };
        if hex(&sha1(&raw)) != manifest {
            return Err("NWSync manifest SHA-1 mismatch".into());
        }
        let rows = nwsync::read_manifest(&raw)?;
        if rows.len() > limits.entries {
            return Err("NWSync manifest exceeds entry limit".into());
        }
        let mut entries = BTreeMap::new();
        let mut total = 0u64;
        for (key, hash, size) in rows {
            total = total.checked_add(u64::from(size)).ok_or("NWSync total size overflow")?;
            if size as usize > limits.resource_bytes || total > limits.total_bytes {
                return Err("NWSync manifest exceeds resource/total size limit".into());
            }
            if entries.insert(key, (hash, size)).is_some() {
                return Err("duplicate NWSync resource name".into());
            }
        }
        for (hash, size) in entries.values() {
            fetch_blob(&blobs, hash, size, fetch)?;
        }
        if !path.exists() {
            atomic(&path, &raw)?;
        }
        Ok(Self { root: root.into(), manifest: manifest.into(), entries })
    }

    /// Four bounded workers; only complete, verified blobs enter the shared cache.
    /// The callback must enforce transport policy and cancellation on reads.
    pub fn fetch_parallel(
        root: &Path,
        manifest: &str,
        limits: Limits,
        fetch: &(impl Fn(&str, usize) -> Result<Box<dyn Read>, String> + Sync),
    ) -> Result<Self, String> {
        digest_name(manifest)?;
        directory(root)?;
        let manifests = root.join("manifests");
        let blobs = root.join("blobs");
        directory(&manifests)?;
        directory(&blobs)?;
        let path = manifests.join(manifest);
        let raw = if path.exists() {
            bounded(File::open(&path).map_err(|e| e.to_string())?, limits.manifest_bytes)?
        } else {
            bounded(
                fetch(&format!("manifests/{manifest}"), limits.manifest_bytes)?,
                limits.manifest_bytes,
            )?
        };
        if hex(&sha1(&raw)) != manifest {
            return Err("NWSync manifest SHA-1 mismatch".into());
        }
        let rows = nwsync::read_manifest(&raw)?;
        if rows.len() > limits.entries {
            return Err("NWSync entry limit".into());
        }
        let mut entries = BTreeMap::new();
        let mut unique = BTreeMap::new();
        let mut total = 0u64;
        for (key, hash, size) in rows {
            total = total.checked_add(u64::from(size)).ok_or("NWSync size overflow")?;
            if size as usize > limits.resource_bytes || total > limits.total_bytes {
                return Err("NWSync resource/total limit".into());
            }
            if entries.insert(key, (hash, size)).is_some() {
                return Err("duplicate resource".into());
            }
            if unique.insert(hash, size).is_some_and(|old| old != size) {
                return Err("inconsistent size".into());
            }
        }
        let tasks = unique.into_iter().collect::<Vec<_>>();
        std::thread::scope(|scope| {
            let workers = (0..4)
                .map(|worker| {
                    let tasks = &tasks;
                    let blobs = &blobs;
                    scope.spawn(move || -> Result<(), String> {
                        for (hash, size) in tasks.iter().skip(worker).step_by(4) {
                            fetch_blob(blobs, hash, size, &mut |path| {
                                fetch(path, (*size as usize).saturating_add(1024 * 1024))
                            })?;
                        }
                        Ok(())
                    })
                })
                .collect::<Vec<_>>();
            for worker in workers {
                worker.join().map_err(|_| "NWSync worker failed")??;
            }
            Ok::<(), String>(())
        })?;
        if !path.exists() {
            atomic(&path, &raw)?;
        }
        Ok(Self { root: root.into(), manifest: manifest.into(), entries })
    }

    pub fn decoded_bytes(&self) -> u64 {
        self.entries.values().map(|(_, size)| u64::from(*size)).sum()
    }

    pub fn manifest(&self) -> &str {
        &self.manifest
    }

    /// Materializes only this module manifest into a new folder. No path from a
    /// manifest is used verbatim. A complete folder is renamed into place, then
    /// opened read-only with the separately verified hak layer attached.
    pub fn open_module(&self, destination: &Path, haks: NwSyncCache) -> Result<Module, String> {
        let mut files = Vec::new();
        for key in self.keys() {
            let name = key.resref.to_lowercase().to_string();
            if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return Err("unsafe NWSync module resource name".into());
            }
            if nwsync::SKIPPED.contains(&key.restype) || key.restype == mg_core::ResType::NCS {
                return Err(
                    "private module manifest contains scripts or private annotations".into()
                );
            }
            // Only known extensions can be reopened as folder resources.
            let filename = key.to_string();
            if crate::folder_key(&filename) != Some(key) {
                return Err("unsupported NWSync module resource type".into());
            }
            files.push((filename, self.read(&key).map_err(|e| e.to_string())?.into_owned()));
        }
        if !self.contains(&Module::info_key()) {
            return Err("NWSync module has no module.ifo".into());
        }
        if destination.exists() {
            if !fs::symlink_metadata(destination).map_err(|e| e.to_string())?.is_dir()
                || fs::read_dir(destination).map_err(|e| e.to_string())?.count() != files.len()
            {
                return Err("invalid cached module directory".into());
            }
            for (name, bytes) in &files {
                verified(&destination.join(name), &sha1(bytes), bytes.len() as u32)?;
            }
            let mut module = Module::open(destination).map_err(|e| e.to_string())?;
            module.info().map_err(|e| e.to_string())?;
            module.nwsync = Some(haks);
            return Ok(module);
        }
        let stage = destination
            .with_file_name(format!("nwsync-{}", hex(&sha1(&fastrand::u128(..).to_le_bytes()))));
        fs::create_dir(&stage).map_err(|e| e.to_string())?;
        let result = (|| {
            for (name, bytes) in &files {
                let mut file =
                    OpenOptions::new().write(true).create_new(true).open(stage.join(name))?;
                file.write_all(bytes)?;
                file.sync_all()?;
            }
            fs::rename(&stage, destination)
        })();
        if let Err(error) = result {
            // This invocation alone created this staging directory and its files.
            let _ = fs::remove_dir_all(&stage);
            return Err(error.to_string());
        }
        for (name, _) in files {
            let path = destination.join(name);
            let mut perms = fs::metadata(&path).map_err(|e| e.to_string())?.permissions();
            perms.set_readonly(true);
            fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
        }
        let mut module = Module::open(destination).map_err(|e| e.to_string())?;
        module.info().map_err(|e| e.to_string())?;
        module.nwsync = Some(haks);
        Ok(module)
    }
}

impl Container for NwSyncCache {
    fn contains(&self, key: &ResKey) -> bool {
        self.entries.contains_key(key)
    }
    fn len(&self) -> usize {
        self.entries.len()
    }
    fn keys(&self) -> Box<dyn Iterator<Item = ResKey> + '_> {
        Box::new(self.entries.keys().copied())
    }
    fn read(&self, key: &ResKey) -> Result<Cow<'_, [u8]>, ResError> {
        let (hash, size) = self.entries.get(key).ok_or(ResError::NotFound(*key))?;
        let path = self.root.join("blobs").join(hex(hash));
        verified(&path, hash, *size)
            .map(Cow::Owned)
            .map_err(|message| ResError::Container { path, message })
    }
}
