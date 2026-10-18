//! Per-server secrets. Only an unavailable OS store permits a private file fallback.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{SessionToken, fingerprint};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("OS credential store unavailable")]
    Unavailable,
    #[error("OS credential store refused access")]
    Denied,
    #[error("Credential storage failed")]
    Storage,
    #[error("Credential fallback is not private or contains a symbolic link")]
    Permissions,
}

/// Injectable for tests; implementations must not include secret values in errors.
pub trait Backend: Send + Sync {
    fn get(&self, server: &str) -> Result<Option<SessionToken>, StoreError>;
    fn set(&self, server: &str, token: &SessionToken) -> Result<(), StoreError>;
    fn delete(&self, server: &str) -> Result<(), StoreError>;
}

#[derive(Debug, Default)]
pub struct OsKeyring;

fn keyring_error(error: keyring::Error) -> StoreError {
    match error {
        keyring::Error::PlatformFailure(_) => StoreError::Unavailable,
        keyring::Error::NoStorageAccess(_) => StoreError::Denied,
        _ => StoreError::Storage,
    }
}

fn entry(server: &str) -> Result<keyring::Entry, StoreError> {
    keyring::Entry::new("org.moonglow.live", server).map_err(keyring_error)
}

impl Backend for OsKeyring {
    fn get(&self, server: &str) -> Result<Option<SessionToken>, StoreError> {
        match entry(server)?.get_password() {
            Ok(value) => SessionToken::new(value).map(Some).map_err(|_| StoreError::Storage),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keyring_error(e)),
        }
    }
    fn set(&self, server: &str, token: &SessionToken) -> Result<(), StoreError> {
        entry(server)?.set_password(token.expose()).map_err(keyring_error)
    }
    fn delete(&self, server: &str) -> Result<(), StoreError> {
        match entry(server)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keyring_error(e)),
        }
    }
}

#[derive(Debug)]
pub struct TokenStore<B = OsKeyring> {
    backend: B,
    directory: PathBuf,
}

impl TokenStore {
    /// `data_dir` must be Moonglow's application data directory, never the game folder.
    pub fn system(data_dir: &Path) -> Self {
        Self::new(OsKeyring, data_dir)
    }
}

impl<B: Backend> TokenStore<B> {
    pub fn new(backend: B, data_dir: &Path) -> Self {
        Self { backend, directory: data_dir.join("live-credentials") }
    }

    fn path(&self, server: &str) -> PathBuf {
        self.directory.join(fingerprint(server.as_bytes()))
    }

    pub fn get(&self, server: &str) -> Result<Option<SessionToken>, StoreError> {
        match self.backend.get(server) {
            Ok(Some(token)) => Ok(Some(token)),
            Ok(None) => {
                let token = self.read_file(server)?;
                if let Some(token) = &token {
                    self.backend.set(server, token)?;
                    self.remove_file(server)?;
                }
                Ok(token)
            }
            Err(StoreError::Unavailable) => self.read_file(server),
            Err(error) => Err(error),
        }
    }

    pub fn set(&self, server: &str, token: &SessionToken) -> Result<(), StoreError> {
        match self.backend.set(server, token) {
            Ok(()) => self.remove_file(server),
            Err(StoreError::Unavailable) => self.write_file(server, token),
            Err(error) => Err(error),
        }
    }

    pub fn delete(&self, server: &str) -> Result<(), StoreError> {
        match self.backend.delete(server) {
            Ok(()) | Err(StoreError::Unavailable) => self.remove_file(server),
            Err(error) => Err(error),
        }
    }

    fn read_file(&self, server: &str) -> Result<Option<SessionToken>, StoreError> {
        let path = self.path(server);
        if !path.try_exists().map_err(|_| StoreError::Storage)? {
            return Ok(None);
        }
        private(&self.directory, true)?;
        private(&path, false)?;
        let metadata = fs::metadata(&path).map_err(|_| StoreError::Storage)?;
        if metadata.len() > 4096 {
            return Err(StoreError::Storage);
        }
        SessionToken::new(fs::read_to_string(path).map_err(|_| StoreError::Storage)?)
            .map(Some)
            .map_err(|_| StoreError::Storage)
    }

    fn write_file(&self, server: &str, token: &SessionToken) -> Result<(), StoreError> {
        if !self.directory.try_exists().map_err(|_| StoreError::Storage)? {
            fs::create_dir_all(self.directory.parent().ok_or(StoreError::Storage)?)
                .map_err(|_| StoreError::Storage)?;
            #[allow(unused_mut)]
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&self.directory).map_err(|_| StoreError::Storage)?;
            #[cfg(windows)]
            secure_windows_directory(&self.directory)?;
        }
        private(&self.directory, true)?;
        let path = self.path(server);
        if path.exists() {
            private(&path, false)?;
        }
        let mut file =
            tempfile::NamedTempFile::new_in(&self.directory).map_err(|_| StoreError::Storage)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| StoreError::Storage)?;
        }
        file.write_all(token.expose().as_bytes()).map_err(|_| StoreError::Storage)?;
        file.as_file().sync_all().map_err(|_| StoreError::Storage)?;
        file.persist(path).map_err(|_| StoreError::Storage)?;
        Ok(())
    }

    fn remove_file(&self, server: &str) -> Result<(), StoreError> {
        let path = self.path(server);
        if !path.try_exists().map_err(|_| StoreError::Storage)? {
            return Ok(());
        }
        private(&self.directory, true)?;
        private(&path, false)?;
        fs::remove_file(path).map_err(|_| StoreError::Storage)
    }
}

fn private(path: &Path, directory: bool) -> Result<(), StoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| StoreError::Storage)?;
    if metadata.file_type().is_symlink() || metadata.is_dir() != directory {
        return Err(StoreError::Permissions);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o777 != if directory { 0o700 } else { 0o600 } {
            return Err(StoreError::Permissions);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(StoreError::Permissions);
        }
        // Reapply a private ACL before any access, including an existing fallback.
        secure_windows_directory(path)?;
    }
    Ok(())
}

#[cfg(windows)]
fn secure_windows_directory(path: &Path) -> Result<(), StoreError> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    // Protected DACL containing only the current user's SID (the Windows 0600 equivalent).
    // Pass the path as data: PowerShell also parses Unicode smart quotes as delimiters.
    let script = " $sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User; $item=Get-Item -LiteralPath $p; $acl=if($item.PSIsContainer){New-Object System.Security.AccessControl.DirectorySecurity}else{New-Object System.Security.AccessControl.FileSecurity}; $acl.SetAccessRuleProtection($true,$false); $inherit=if((Get-Item -LiteralPath $p).PSIsContainer){[System.Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'}else{[System.Security.AccessControl.InheritanceFlags]::None}; $rule=New-Object System.Security.AccessControl.FileSystemAccessRule($sid,'FullControl',$inherit,'None','Allow'); $acl.AddAccessRule($rule); $item.SetAccessControl($acl)";
    use base64::Engine;
    let script = format!("$ErrorActionPreference='Stop'; $p=$env:MG_ACL_PATH; {script}");
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .env("MG_ACL_PATH", path)
        .creation_flags(0x08000000)
        .output()
        .map_err(|_| StoreError::Permissions)?;
    if !output.status.success() {
        return Err(StoreError::Permissions);
    }
    Ok(())
}
