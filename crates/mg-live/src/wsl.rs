//! One service-owned Windows WSL keepalive; never searches for or kills a PID.
use std::{process::Child, sync::Mutex};

#[derive(Debug, Default)]
pub(crate) struct KeepWsl {
    child: Mutex<Option<Child>>,
    owner: String,
}
impl KeepWsl {
    pub(crate) fn start(enabled: bool) -> Result<Self, String> {
        #[allow(unused_mut)]
        let mut owner = Self::default();
        #[cfg(windows)]
        if enabled {
            use std::{
                os::windows::process::CommandExt,
                process::{Command, Stdio},
            };
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            owner.owner = format!(
                "{}-{}-{}",
                std::process::id(),
                crate::now_ms(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            let child = Command::new("wsl.exe")
                .args(["-e", "sleep", "infinity"])
                .env("MOONGLOW_KEEP_OWNER", &owner.owner)
                .env(
                    "WSLENV",
                    format!("{}:MOONGLOW_KEEP_OWNER", std::env::var("WSLENV").unwrap_or_default()),
                )
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .spawn()
                .map_err(|e| format!("Starting WSL keepalive: {e}"))?;
            *owner.child.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
        }
        #[cfg(not(windows))]
        let _ = enabled;
        Ok(owner)
    }
    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }
    pub(crate) fn pid(&self) -> Option<u32> {
        let mut guard = self.child.lock().unwrap_or_else(|e| e.into_inner());
        let child = guard.as_mut()?;
        matches!(child.try_wait(), Ok(None)).then(|| child.id())
    }
    pub(crate) fn stop(&mut self) {
        if let Some(mut child) = self.child.get_mut().unwrap_or_else(|e| e.into_inner()).take() {
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}
impl Drop for KeepWsl {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_creates_no_child() {
        assert!(KeepWsl::start(false).unwrap().pid().is_none());
    }
}
