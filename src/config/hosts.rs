//! `hosts.yml` — per-host auth state. Schema details live in spec 02; this slice
//! provides the load/save/get/set plumbing that the auth slice will fill in.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_yaml::{Mapping, Value};

use super::{value_to_string, yaml};

#[derive(Debug, Clone, Default)]
pub struct Hosts {
    data: Mapping,
    path: PathBuf,
}

impl Hosts {
    pub async fn load_from(path: &Path) -> Result<Self> {
        let data = yaml::load_mapping(path).await?;
        Ok(Self {
            data,
            path: path.to_path_buf(),
        })
    }

    /// Read a scalar key under a host block.
    pub fn get(&self, host: &str, key: &str) -> Option<String> {
        let host_block = self.data.get(host)?;
        let host_map = match host_block {
            Value::Mapping(m) => m,
            _ => return None,
        };
        host_map.get(key).and_then(value_to_string)
    }

    /// List the hostnames currently recorded.
    pub fn hosts(&self) -> Vec<String> {
        self.data.keys().filter_map(value_to_string).collect()
    }

    /// Get the raw mapping for a host (or an empty mapping if absent).
    pub fn host_block(&self, host: &str) -> Mapping {
        match self.data.get(host) {
            Some(Value::Mapping(m)) => m.clone(),
            _ => Mapping::new(),
        }
    }

    /// Set a key under a host. Creates the host block if missing. Persists immediately.
    pub async fn set(&mut self, host: &str, key: &str, value: &str) -> Result<()> {
        let key_v = Value::String(key.into());
        let value_v = Value::String(value.into());
        match self.data.get_mut(host) {
            Some(Value::Mapping(m)) => {
                m.insert(key_v, value_v);
            }
            _ => {
                let mut m = Mapping::new();
                m.insert(key_v, value_v);
                self.data
                    .insert(Value::String(host.into()), Value::Mapping(m));
            }
        }
        yaml::save_mapping(&self.path, &self.data).await
    }

    /// Remove a host entry entirely. Persists immediately.
    pub async fn remove_host(&mut self, host: &str) -> Result<()> {
        self.data.remove(host);
        yaml::save_mapping(&self.path, &self.data).await
    }

    /// Emit the YAML for one host block (used by `bb config list --host`).
    pub fn host_yaml(&self, host: &str) -> Result<String> {
        let block = Value::Mapping(self.host_block(host));
        Ok(serde_yaml::to_string(&block)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn load_missing_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let h = Hosts::load_from(&dir.path().join("hosts.yml"))
            .await
            .unwrap();
        assert!(h.hosts().is_empty());
    }

    #[tokio::test]
    async fn set_then_get_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("hosts.yml");
        let mut h = Hosts::load_from(&path).await.unwrap();
        h.set("bitbucket.org", "active_user", "hbackman")
            .await
            .unwrap();
        h.set("bitbucket.org", "git_protocol", "ssh").await.unwrap();
        // Reload from disk to verify persistence.
        let h2 = Hosts::load_from(&path).await.unwrap();
        assert_eq!(
            h2.get("bitbucket.org", "active_user").as_deref(),
            Some("hbackman")
        );
        assert_eq!(
            h2.get("bitbucket.org", "git_protocol").as_deref(),
            Some("ssh")
        );
    }

    #[tokio::test]
    async fn unknown_host_yields_none() {
        let dir = TempDir::new().unwrap();
        let h = Hosts::load_from(&dir.path().join("hosts.yml"))
            .await
            .unwrap();
        assert_eq!(h.get("bitbucket.org", "active_user"), None);
    }

    #[tokio::test]
    async fn preserves_unknown_keys_under_host() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("hosts.yml");
        // Pre-seed a file with an unknown nested key, then mutate via set().
        let seed = "bitbucket.org:\n  active_user: hbackman\n  future_key: keepme\n";
        tokio::fs::write(&path, seed).await.unwrap();
        let mut h = Hosts::load_from(&path).await.unwrap();
        h.set("bitbucket.org", "git_protocol", "https")
            .await
            .unwrap();
        let h2 = Hosts::load_from(&path).await.unwrap();
        assert_eq!(
            h2.get("bitbucket.org", "future_key").as_deref(),
            Some("keepme")
        );
        assert_eq!(
            h2.get("bitbucket.org", "git_protocol").as_deref(),
            Some("https")
        );
    }
}
