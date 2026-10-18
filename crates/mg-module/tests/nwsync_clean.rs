//! Authored-only acceptance fixture: a clean client has no local game or hak files.
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mg_core::{
    ResRef, ResType,
    sha1::{hex, sha1},
};
use mg_erf::{
    ErfWriter,
    compressedbuf::{MAGIC_NSYC, compress, decompress_limited},
};
use mg_gff::{Gff, Struct, Value};
use mg_module::{
    Module,
    nwsync::{self, Options, Source},
    nwsync_cache::{Limits, NwSyncCache},
};
use mg_resman::{Container, GameInstall, LayerClass, ResKey, ResMan};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("mg-nwsync-clean-{:032x}", fastrand::u128(..)));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        // Only files this test created, including explicitly read-only module resources.
        fn writable(path: &Path) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    writable(&path);
                } else {
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::{MetadataExt, PermissionsExt};
                        let attrs = fs::metadata(&path).unwrap().file_attributes();
                        let perms = fs::Permissions::from_attributes(attrs & !1);
                        fs::set_permissions(&path, perms).unwrap();
                    }
                }
            }
        }
        writable(&self.0);
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn key(name: &str, typ: ResType) -> ResKey {
    ResKey::parse(name, typ).unwrap()
}
fn fetcher(repo: &Path) -> impl FnMut(&str) -> Result<Box<dyn Read>, String> + '_ {
    move |path| {
        fs::File::open(repo.join(path))
            .map(|f| Box::new(f) as Box<dyn Read>)
            .map_err(|e| e.to_string())
    }
}

#[test]
fn clean_client_resolves_ordered_haks_without_local_haks_and_rejects_tampering() {
    let scratch = Scratch::new();
    let server = scratch.0.join("server");
    fs::create_dir_all(server.join("hak")).unwrap();
    for (name, bytes) in [("first", b"first hak"), ("second", b"other hak")] {
        let mut hak = ErfWriter::new(*b"HAK ");
        hak.add(ResRef::from_str("collision").unwrap(), ResType::TXT, bytes).unwrap();
        hak.add(ResRef::from_str(name).unwrap(), ResType::TXT, bytes).unwrap();
        fs::write(server.join("hak").join(format!("{name}.hak")), hak.to_bytes().unwrap()).unwrap();
    }
    let mut module = Module::new();
    let mut info = Gff::new(*b"IFO ");
    info.root.set(
        "Mod_HakList",
        Value::List(
            ["first", "second"]
                .map(|name| {
                    let mut row = Struct::new(0);
                    row.set("Mod_Hak", Value::String(name.as_bytes().into()));
                    row
                })
                .to_vec(),
        ),
    );
    module.set_info(&info).unwrap();
    module.set(key("collision", ResType::TXT), &b"module loses to hak"[..]);
    module.set(key("only_module", ResType::TXT), &b"module content"[..]);
    for typ in [ResType::NSS, ResType::NCS, ResType::NDB, ResType::GIC] {
        module.set(key("private", typ), &b"private data"[..]);
    }
    let repo = scratch.0.join("repository");
    let server_install = GameInstall::new(server.join("no-install"), Some(server), "en");
    let (haks, missing) =
        nwsync::module_contents(&module, &server_install, &ResMan::new(), false, None).unwrap();
    assert!(missing.is_empty());
    let opts = Options { limit: Some(nwsync::FILE_LIMIT), ..Default::default() };
    let hak_manifest = nwsync::write(&repo, &haks, &opts, &mut |_, _| {}).unwrap();
    let private = nwsync::write(
        &repo,
        &nwsync::private_module_contents(&module),
        &Options { with_module: true, ..opts.clone() },
        &mut |_, _| {},
    )
    .unwrap();
    let cache = scratch.0.join("cache");
    let haks =
        NwSyncCache::fetch(&cache, &hak_manifest.sha1, Limits::default(), &mut fetcher(&repo))
            .unwrap();
    let module_cache =
        NwSyncCache::fetch(&cache, &private.sha1, Limits::default(), &mut fetcher(&repo)).unwrap();
    assert_eq!(module_cache.len(), 3);
    assert!(!module_cache.contains(&key("second", ResType::TXT)));
    let folder = scratch.0.join("module");
    let mut opened = module_cache.open_module(&folder, haks.clone()).unwrap();
    assert_eq!(opened.haks().unwrap(), ["first", "second"]);
    assert_eq!(opened.get(&Module::info_key()), module.get(&Module::info_key()));
    assert!(opened.save().unwrap_err().to_string().contains("read-only"));
    assert!(fs::metadata(folder.join("module.ifo")).unwrap().permissions().readonly());
    let clean = GameInstall::new(scratch.0.join("absent-game"), None, "en");
    let mut resman = ResMan::new();
    let (missing, error) = opened.stack_onto(&mut resman, &clean);
    assert!(missing.is_empty() && error.is_none());
    assert_eq!(&*resman.get(&key("second", ResType::TXT)).unwrap(), b"other hak");
    assert_eq!(&*resman.get(&key("collision", ResType::TXT)).unwrap(), b"first hak");
    assert_eq!(resman.layers()[0].class, LayerClass::NwSync);
    assert_eq!(&*resman.get(&key("only_module", ResType::TXT)).unwrap(), b"module content");
    // Changing the module's order changes the hak manifest's winning collision.
    let mut reverse = info.clone();
    reverse.root.set(
        "Mod_HakList",
        Value::List(
            ["second", "first"]
                .map(|name| {
                    let mut row = Struct::new(0);
                    row.set("Mod_Hak", Value::String(name.as_bytes().into()));
                    row
                })
                .to_vec(),
        ),
    );
    module.set_info(&reverse).unwrap();
    let (reverse_haks, _) =
        nwsync::module_contents(&module, &server_install, &ResMan::new(), false, None).unwrap();
    let reversed = nwsync::write(&repo, &reverse_haks, &opts, &mut |_, _| {}).unwrap();
    let reversed =
        NwSyncCache::fetch(&cache, &reversed.sha1, Limits::default(), &mut fetcher(&repo)).unwrap();
    assert_eq!(&*reversed.read(&key("collision", ResType::TXT)).unwrap(), b"other hak");
    // Cached content is checked again on reads, not trusted merely because its name matches.
    let digest = hex(&sha1(b"other hak"));
    fs::write(cache.join("blobs").join(&digest), b"bad bytes").unwrap();
    assert!(haks.read(&key("second", ResType::TXT)).is_err());
    assert!(
        NwSyncCache::fetch(&cache, &hak_manifest.sha1, Limits::default(), &mut fetcher(&repo))
            .is_err()
    );
    // A corrupted download is rejected before an atomic blob or manifest is installed.
    fs::write(
        repo.join("data/sha1").join(&digest[..2]).join(&digest[2..4]).join(&digest),
        compress(b"bad bytes", MAGIC_NSYC),
    )
    .unwrap();
    let bad_cache = scratch.0.join("bad-cache");
    assert!(
        NwSyncCache::fetch(&bad_cache, &hak_manifest.sha1, Limits::default(), &mut fetcher(&repo))
            .is_err()
    );
    assert!(!bad_cache.join("blobs").join(&digest).exists());
    assert!(!bad_cache.join("manifests").join(&hak_manifest.sha1).exists());
    for entry in fs::read_dir(cache.join("blobs")).unwrap() {
        let name = entry.unwrap().file_name().to_str().unwrap().to_owned();
        assert_eq!(name.len(), 40);
        assert!(name.bytes().all(|b| b.is_ascii_hexdigit()));
    }
}

#[test]
fn untrusted_manifest_names_sizes_and_compressed_expansion_are_bounded() {
    let scratch = Scratch::new();
    let cache = scratch.0.join("cache");
    let mut no_fetch =
        |_: &str| -> Result<Box<dyn Read>, String> { panic!("invalid digest fetched") };
    assert!(
        NwSyncCache::fetch(&cache, "../not-a-digest", Limits::default(), &mut no_fetch).is_err()
    );
    let mut bomb = compress(&vec![b'x'; 200_000], MAGIC_NSYC);
    assert!(decompress_limited(&bomb, MAGIC_NSYC, 128).is_err());
    bomb[12..16].copy_from_slice(&128u32.to_le_bytes());
    assert!(decompress_limited(&bomb, MAGIC_NSYC, 128).is_err());
    let raw = nwsync::manifest(&[(key("huge", ResType::TXT), [0; 20], u32::MAX)]);
    let id = hex(&sha1(&raw));
    let mut fetch = |_: &str| Ok(Box::new(std::io::Cursor::new(raw.clone())) as Box<dyn Read>);
    assert!(NwSyncCache::fetch(&cache, &id, Limits::default(), &mut fetch).is_err());
    for prefix in 0..raw.len() {
        assert!(nwsync::read_manifest(&raw[..prefix]).is_err());
    }
    let repo = scratch.0.join("repo");
    let mut module = Module::new();
    module.set_info(&Gff::new(*b"IFO ")).unwrap();
    let mut contents = nwsync::private_module_contents(&module);
    contents.push((key("../escape", ResType::TXT), Source::Bytes(Arc::from(&b"bad"[..]))));
    let written = nwsync::write(&repo, &contents, &Options::default(), &mut |_, _| {}).unwrap();
    let loaded =
        NwSyncCache::fetch(&cache, &written.sha1, Limits::default(), &mut fetcher(&repo)).unwrap();
    assert!(loaded.open_module(&scratch.0.join("module"), loaded.clone()).is_err());
    assert!(!scratch.0.join("module").exists());
}
