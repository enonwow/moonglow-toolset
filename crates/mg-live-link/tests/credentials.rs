use std::collections::HashMap;
use std::sync::Mutex;

use mg_live_link::SessionToken;
use mg_live_link::credentials::{Backend, StoreError, TokenStore};

#[derive(Default)]
struct Mock {
    tokens: Mutex<HashMap<String, SessionToken>>,
    failure: Option<StoreError>,
}

impl Backend for Mock {
    fn get(&self, server: &str) -> Result<Option<SessionToken>, StoreError> {
        if let Some(e) = self.failure {
            return Err(e);
        }
        Ok(self.tokens.lock().unwrap().get(server).cloned())
    }
    fn set(&self, server: &str, token: &SessionToken) -> Result<(), StoreError> {
        if let Some(e) = self.failure {
            return Err(e);
        }
        self.tokens.lock().unwrap().insert(server.into(), token.clone());
        Ok(())
    }
    fn delete(&self, server: &str) -> Result<(), StoreError> {
        if let Some(e) = self.failure {
            return Err(e);
        }
        self.tokens.lock().unwrap().remove(server);
        Ok(())
    }
}

#[test]
#[cfg(windows)]
fn fallback_handles_smart_quote_in_directory_name() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("builder\u{2019}s data");
    let store = TokenStore::new(
        Mock { failure: Some(StoreError::Unavailable), ..Default::default() },
        &data,
    );
    let token = SessionToken::new("fixture-session".into()).unwrap();
    store.set("one", &token).unwrap();
    assert_eq!(store.get("one").unwrap(), Some(token));
    store.delete("one").unwrap();
}

#[test]
fn credentials_are_separate_per_server_and_never_written_when_keyring_works() {
    let dir = tempfile::tempdir().unwrap();
    let store = TokenStore::new(Mock::default(), dir.path());
    let token = SessionToken::new("fixture-session".into()).unwrap();
    store.set("one", &token).unwrap();
    assert_eq!(store.get("one").unwrap(), Some(token));
    assert_eq!(store.get("two").unwrap(), None);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    store.delete("one").unwrap();
    assert_eq!(store.get("one").unwrap(), None);
}

#[test]
fn unavailable_falls_back_privately_then_migrates_back_to_keyring() {
    let dir = tempfile::tempdir().unwrap();
    let store = TokenStore::new(
        Mock { failure: Some(StoreError::Unavailable), ..Default::default() },
        dir.path(),
    );
    let token = SessionToken::new("fixture-session".into()).unwrap();
    store.set("../server", &token).unwrap();
    let directory = dir.path().join("live-credentials");
    let file = std::fs::read_dir(&directory).unwrap().next().unwrap().unwrap().path();
    assert_eq!(file.parent(), Some(directory.as_path()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    }
    assert_eq!(store.get("../server").unwrap(), Some(token.clone()));
    let recovered = TokenStore::new(Mock::default(), dir.path());
    assert_eq!(recovered.get("../server").unwrap(), Some(token.clone()));
    assert!(!file.exists());
    assert_eq!(recovered.get("../server").unwrap(), Some(token));
    recovered.delete("../server").unwrap();
    assert_eq!(recovered.get("../server").unwrap(), None);
}

#[test]
fn denied_access_does_not_silently_create_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = TokenStore::new(
        Mock { failure: Some(StoreError::Denied), ..Default::default() },
        dir.path(),
    );
    assert_eq!(
        store.set("one", &SessionToken::new("fixture".into()).unwrap()),
        Err(StoreError::Denied)
    );
    assert_eq!(store.get("one"), Err(StoreError::Denied));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn fallback_overwrite_and_sign_out_remove_the_secret() {
    let dir = tempfile::tempdir().unwrap();
    let store = TokenStore::new(
        Mock { failure: Some(StoreError::Unavailable), ..Default::default() },
        dir.path(),
    );
    store.set("one", &SessionToken::new("first".into()).unwrap()).unwrap();
    let second = SessionToken::new("second".into()).unwrap();
    store.set("one", &second).unwrap();
    assert_eq!(store.get("one").unwrap(), Some(second));
    store.delete("one").unwrap();
    assert_eq!(store.get("one").unwrap(), None);
}
