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

    /// Emit the YAML for one host block (used by `bbk config list --host`).
    pub fn host_yaml(&self, host: &str) -> Result<String> {
        let block = Value::Mapping(self.host_block(host));
        Ok(serde_yaml::to_string(&block)?)
    }

    /// Names of users recorded under `<host>.users:`.
    pub fn users(&self, host: &str) -> Vec<String> {
        let block = self.host_block(host);
        match block.get("users") {
            Some(Value::Mapping(m)) => m.keys().filter_map(value_to_string).collect(),
            _ => Vec::new(),
        }
    }

    /// Mapping stored under `<host>.users.<user>` (empty mapping if absent).
    pub fn user_block(&self, host: &str, user: &str) -> Mapping {
        let host_block = self.host_block(host);
        match host_block.get("users") {
            Some(Value::Mapping(users)) => match users.get(user) {
                Some(Value::Mapping(m)) => m.clone(),
                _ => Mapping::new(),
            },
            _ => Mapping::new(),
        }
    }

    /// Replace (or insert) the user block under `<host>.users.<user>`.
    pub async fn set_user_block(&mut self, host: &str, user: &str, block: Mapping) -> Result<()> {
        let host_v = Value::String(host.into());
        let users_v = Value::String("users".into());
        let user_v = Value::String(user.into());

        let host_map = match self.data.get_mut(&host_v) {
            Some(Value::Mapping(m)) => m,
            _ => {
                self.data
                    .insert(host_v.clone(), Value::Mapping(Mapping::new()));
                if let Some(Value::Mapping(m)) = self.data.get_mut(&host_v) {
                    m
                } else {
                    unreachable!()
                }
            }
        };

        let users_map = match host_map.get_mut(&users_v) {
            Some(Value::Mapping(m)) => m,
            _ => {
                host_map.insert(users_v.clone(), Value::Mapping(Mapping::new()));
                if let Some(Value::Mapping(m)) = host_map.get_mut(&users_v) {
                    m
                } else {
                    unreachable!()
                }
            }
        };

        users_map.insert(user_v, Value::Mapping(block));
        yaml::save_mapping(&self.path, &self.data).await
    }

    /// Remove `<host>.users.<user>`. Persists immediately.
    pub async fn remove_user(&mut self, host: &str, user: &str) -> Result<()> {
        if let Some(Value::Mapping(host_map)) = self.data.get_mut(host) {
            if let Some(Value::Mapping(users)) = host_map.get_mut("users") {
                users.remove(user);
            }
        }
        yaml::save_mapping(&self.path, &self.data).await
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
    async fn user_block_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("hosts.yml");
        let mut h = Hosts::load_from(&path).await.unwrap();
        let mut block = Mapping::new();
        block.insert(Value::String("type".into()), Value::String("oauth".into()));
        block.insert(
            Value::String("git_protocol".into()),
            Value::String("ssh".into()),
        );
        h.set_user_block("bitbucket.org", "hbackman", block.clone())
            .await
            .unwrap();
        let reloaded = Hosts::load_from(&path).await.unwrap();
        assert_eq!(reloaded.user_block("bitbucket.org", "hbackman"), block);
        assert_eq!(
            reloaded.users("bitbucket.org"),
            vec!["hbackman".to_string()]
        );
    }

    #[tokio::test]
    async fn remove_user_keeps_other_users() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("hosts.yml");
        let mut h = Hosts::load_from(&path).await.unwrap();
        let mut b = Mapping::new();
        b.insert(Value::String("type".into()), Value::String("oauth".into()));
        h.set_user_block("bitbucket.org", "alice", b.clone())
            .await
            .unwrap();
        h.set_user_block("bitbucket.org", "bob", b.clone())
            .await
            .unwrap();
        h.remove_user("bitbucket.org", "alice").await.unwrap();
        let reloaded = Hosts::load_from(&path).await.unwrap();
        let users = reloaded.users("bitbucket.org");
        assert_eq!(users, vec!["bob".to_string()]);
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
