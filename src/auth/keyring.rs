//! OS keyring abstraction. Production uses the `keyring` crate (macOS Keychain,
//! Linux Secret Service, Windows Credential Manager); tests use the in-memory
//! `MemKeyring`.
//!
//! Keyring "service" is `bb:<host>`; "username" is the Bitbucket account name.
//! The stored value is a JSON `KeyringBlob` from `crate::auth::KeyringBlob`.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::{anyhow, Result};

/// Result distinguishing "not found" from real errors. Real errors include
/// "no backend available" — callers fall back to plaintext on backend errors,
/// not on not-found.
#[derive(Debug, thiserror::Error)]
pub enum KeyringError {
    #[error("entry not found")]
    NotFound,
    #[error("{0}")]
    Backend(String),
}

impl From<anyhow::Error> for KeyringError {
    fn from(e: anyhow::Error) -> Self {
        KeyringError::Backend(e.to_string())
    }
}

pub trait KeyringBackend: Send + Sync + std::fmt::Debug {
    fn set_password(&self, host: &str, user: &str, blob: &str) -> Result<(), KeyringError>;
    fn get_password(&self, host: &str, user: &str) -> Result<String, KeyringError>;
    fn delete_password(&self, host: &str, user: &str) -> Result<(), KeyringError>;
}

fn service_for(host: &str) -> String {
    format!("bb:{host}")
}

/// Real OS keyring backed by the `keyring` crate.
#[derive(Debug, Default)]
pub struct SystemKeyring;

impl SystemKeyring {
    fn entry(host: &str, user: &str) -> Result<::keyring::Entry, KeyringError> {
        let service = service_for(host);
        ::keyring::Entry::new(&service, user).map_err(|e| KeyringError::Backend(e.to_string()))
    }
}

impl KeyringBackend for SystemKeyring {
    fn set_password(&self, host: &str, user: &str, blob: &str) -> Result<(), KeyringError> {
        Self::entry(host, user)?
            .set_password(blob)
            .map_err(|e| KeyringError::Backend(e.to_string()))
    }

    fn get_password(&self, host: &str, user: &str) -> Result<String, KeyringError> {
        Self::entry(host, user)?
            .get_password()
            .map_err(|e| match e {
                ::keyring::Error::NoEntry => KeyringError::NotFound,
                other => KeyringError::Backend(other.to_string()),
            })
    }

    fn delete_password(&self, host: &str, user: &str) -> Result<(), KeyringError> {
        match Self::entry(host, user)?.delete_credential() {
            Ok(_) => Ok(()),
            Err(::keyring::Error::NoEntry) => Err(KeyringError::NotFound),
            Err(other) => Err(KeyringError::Backend(other.to_string())),
        }
    }
}

/// In-memory backend, used by tests and as a fallback when the OS backend is
/// unavailable but the caller has explicitly opted into the plaintext path.
#[derive(Debug, Default)]
pub struct MemKeyring {
    inner: Mutex<HashMap<(String, String), String>>,
}

impl MemKeyring {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KeyringBackend for MemKeyring {
    fn set_password(&self, host: &str, user: &str, blob: &str) -> Result<(), KeyringError> {
        self.inner
            .lock()
            .map_err(|e| KeyringError::Backend(e.to_string()))?
            .insert((host.to_string(), user.to_string()), blob.to_string());
        Ok(())
    }

    fn get_password(&self, host: &str, user: &str) -> Result<String, KeyringError> {
        self.inner
            .lock()
            .map_err(|e| KeyringError::Backend(e.to_string()))?
            .get(&(host.to_string(), user.to_string()))
            .cloned()
            .ok_or(KeyringError::NotFound)
    }

    fn delete_password(&self, host: &str, user: &str) -> Result<(), KeyringError> {
        let removed = self
            .inner
            .lock()
            .map_err(|e| KeyringError::Backend(e.to_string()))?
            .remove(&(host.to_string(), user.to_string()));
        if removed.is_some() {
            Ok(())
        } else {
            Err(KeyringError::NotFound)
        }
    }
}

/// Distinguish "no such entry" from "backend errored". Used by the hosts.rs read
/// path to decide whether to fall back to plaintext or surface an error.
pub fn is_not_found(err: &KeyringError) -> bool {
    matches!(err, KeyringError::NotFound)
}

/// Map a generic error into a `KeyringError::Backend`.
pub fn backend_err(msg: impl ToString) -> KeyringError {
    KeyringError::Backend(msg.to_string())
}

/// Convenience: wrap a closure that returns `Result<T, KeyringError>` so callers
/// can fall back to plaintext on backend errors without losing the not-found case.
pub fn ignore_not_found<T>(r: Result<T, KeyringError>) -> Result<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(KeyringError::NotFound) => Ok(None),
        Err(e) => Err(anyhow!("{e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_round_trip() {
        let k = MemKeyring::new();
        assert!(matches!(
            k.get_password("h", "u").unwrap_err(),
            KeyringError::NotFound
        ));
        k.set_password("h", "u", "secret").unwrap();
        assert_eq!(k.get_password("h", "u").unwrap(), "secret");
        k.delete_password("h", "u").unwrap();
        assert!(matches!(
            k.get_password("h", "u").unwrap_err(),
            KeyringError::NotFound
        ));
    }

    #[test]
    fn mem_isolates_users() {
        let k = MemKeyring::new();
        k.set_password("bitbucket.org", "alice", "a").unwrap();
        k.set_password("bitbucket.org", "bob", "b").unwrap();
        assert_eq!(k.get_password("bitbucket.org", "alice").unwrap(), "a");
        assert_eq!(k.get_password("bitbucket.org", "bob").unwrap(), "b");
    }
}
