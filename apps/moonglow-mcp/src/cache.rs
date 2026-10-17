//! Disposable descriptions only: no bearer tokens, endpoints, or request/event payloads.
use crate::{config::data_dir, https};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
#[derive(Serialize, Deserialize)]
struct Entry {
    format: u8,
    descriptor: String,
    etag: String,
    describe: Value,
}
pub fn hash(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn path(key: &str) -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("live/agent-cache").join(format!("{key}.json")))
}
pub fn load(key: &str, descriptor: &str) -> Option<(Value, String)> {
    let path = path(key)?;
    if fs::metadata(&path).ok()?.len() > https::LIMIT as u64 {
        return None;
    }
    let entry: Entry = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    (entry.format == 1 && entry.descriptor == descriptor && https::valid_etag(&entry.etag))
        .then_some((entry.describe, entry.etag))
}
pub fn save(key: &str, descriptor: &str, describe: &Value, etag: &str) {
    let Some(path) = path(key) else { return };
    let Some(parent) = path.parent() else { return };
    let entry = Entry {
        format: 1,
        descriptor: descriptor.into(),
        describe: describe.clone(),
        etag: etag.into(),
    };
    let Ok(bytes) = serde_json::to_vec(&entry) else { return };
    if bytes.len() > https::LIMIT || fs::create_dir_all(parent).is_err() {
        return;
    }
    let temporary = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let Ok(mut file) = options.open(&temporary) {
        let written = file.write_all(&bytes).is_ok() && file.sync_all().is_ok();
        drop(file);
        if written {
            let _ = fs::rename(&temporary, path);
        }
        let _ = fs::remove_file(temporary);
    }
}
