//! Atomic discovery records; the bearer credential never appears in Debug/logs.
use serde_json::json;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(crate) struct Descriptor {
    path: PathBuf,
    pin: String,
}

impl Descriptor {
    pub(crate) fn write(
        data_dir: &Path,
        url: &str,
        token: &str,
        certificate: &[u8],
    ) -> Result<Self, String> {
        let dir = data_dir.join("live/sessions");
        fs::create_dir_all(&dir).map_err(|e| format!("Live descriptor directory: {e}"))?;
        let pid = std::process::id();
        let path = dir.join(format!("{pid}.json"));
        let pin = ring::digest::digest(&ring::digest::SHA256, certificate)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let body = json!({"protocol":"moonglow-agent/1","url":url,"token":token,"certSha256":pin,
            "pid":pid,"startedUnixMs":crate::now_ms(),"version":env!("CARGO_PKG_VERSION")});
        let temp = dir.join(format!("{pid}-{}.tmp", crate::service::generate_token()?));
        let result = (|| -> std::io::Result<()> {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp)?;
            file.write_all(body.to_string().as_bytes())?;
            file.sync_all()?;
            drop(file);
            // A former process may have left this same PID's record after a crash.
            // Removing it first also supports Windows rename semantics; readers
            // can see absence, never a partially written JSON document.
            if path.exists() {
                let old: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
                if old["protocol"] != "moonglow-agent/1" || old["pid"] != pid {
                    return Err(std::io::Error::other("Unrecognized existing descriptor"));
                }
                fs::remove_file(&path)?;
            }
            fs::rename(&temp, &path)
        })();
        if let Err(e) = result {
            let _ = fs::remove_file(&temp);
            return Err(format!("Live descriptor: {e}"));
        }
        Ok(Self { path, pin })
    }
    pub(crate) fn remove(&mut self) -> Result<(), String> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("Live descriptor removal: {e}")),
        };
        let current: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("Live descriptor removal: {e}"))?;
        if current["pid"] == std::process::id() && current["certSha256"] == self.pin {
            fs::remove_file(&self.path).map_err(|e| format!("Live descriptor removal: {e}"))?;
        }
        Ok(())
    }
}
impl Drop for Descriptor {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn record_is_complete_private_and_only_its_owner_removes_it() {
        let dir = std::env::temp_dir().join(format!(
            "mg-live-descriptor-{}-{}",
            std::process::id(),
            crate::now_ms()
        ));
        let first =
            Descriptor::write(&dir, "https://127.0.0.1:9", "first token", b"first").unwrap();
        let path = first.path.clone();
        let body: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(body["protocol"], "moonglow-agent/1");
        assert_eq!(body["token"], "first token");
        assert_eq!(body["certSha256"].as_str().unwrap().len(), 64);
        assert!(!format!("{first:?}").contains("first token"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let second =
            Descriptor::write(&dir, "https://127.0.0.1:10", "second token", b"second").unwrap();
        drop(first);
        assert!(path.exists(), "old service must not remove a replacement");
        drop(second);
        assert!(!path.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
