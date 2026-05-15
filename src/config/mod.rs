//! `~/.config/bbk/config.yml` — user preferences. Loaded once per invocation,
//! cached in `Context.config`.
//!
//! Writes are atomic and preserve unknown keys (so a future `bbk` version's keys
//! aren't clobbered by an older binary).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_yaml::{Mapping, Value};
use tokio::sync::RwLock;

pub mod hosts;
pub mod yaml;

pub use hosts::Hosts;

/// Default Bitbucket Cloud hostname.
pub const DEFAULT_HOST: &str = "bitbucket.org";

const CONFIG_FILE: &str = "config.yml";
const HOSTS_FILE: &str = "hosts.yml";

/// Keys recognized by `config.yml`. Used for validation (`bbk config set` rejects
/// unknown keys) and `bbk config list` (defaults filled in for missing keys).
pub const KNOWN_KEYS: &[&str] = &[
    "default_host",
    "default_repo",
    "git_protocol",
    "editor",
    "pager",
    "browser",
    "prompt",
];

/// Resolve the bbk config directory.
///
/// Precedence:
/// 1. `BB_CONFIG_DIR`
/// 2. `$XDG_CONFIG_HOME/bbk` (Unix)
/// 3. `$HOME/.config/bbk` (Unix)
/// 4. `directories::ProjectDirs` (Windows / fallback)
pub fn config_dir() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("BB_CONFIG_DIR") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    #[cfg(unix)]
    {
        if let Some(p) = std::env::var_os("XDG_CONFIG_HOME") {
            if !p.is_empty() {
                return Ok(PathBuf::from(p).join("bbk"));
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            if !home.is_empty() {
                return Ok(PathBuf::from(home).join(".config").join("bbk"));
            }
        }
    }
    let dirs = directories::ProjectDirs::from("", "", "bbk")
        .ok_or_else(|| anyhow!("could not determine bbk config dir"))?;
    Ok(dirs.config_dir().to_path_buf())
}

#[derive(Debug, Clone)]
pub struct Config {
    data: Mapping,
    path: PathBuf,
    hosts: Arc<RwLock<Hosts>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data: Mapping::new(),
            path: PathBuf::new(),
            hosts: Arc::new(RwLock::new(Hosts::default())),
        }
    }
}

impl Config {
    /// Read both `config.yml` and `hosts.yml`. Missing files return an empty config.
    pub async fn load() -> Result<Self> {
        let dir = config_dir()?;
        Self::load_from(&dir).await
    }

    /// Read both files from a specific directory. Used by tests with `BB_CONFIG_DIR`.
    pub async fn load_from(dir: &Path) -> Result<Self> {
        let config_path = dir.join(CONFIG_FILE);
        let data = yaml::load_mapping(&config_path).await?;
        let hosts = Hosts::load_from(&dir.join(HOSTS_FILE)).await?;
        Ok(Self {
            data,
            path: config_path,
            hosts: Arc::new(RwLock::new(hosts)),
        })
    }

    /// Raw stored value at `key`. Returns `None` for missing or non-scalar values.
    pub fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).and_then(value_to_string)
    }

    /// Stored value with the hardcoded default applied. Defaults to `""` if there is no
    /// documented default for the key.
    pub fn get_or_default(&self, key: &str) -> String {
        self.get(key)
            .unwrap_or_else(|| default_for(key).to_string())
    }

    /// Aliases mapping — accepted but unused in MVP. Returns empty if absent.
    pub fn aliases(&self) -> BTreeMap<String, String> {
        match self.data.get("aliases") {
            Some(Value::Mapping(m)) => m
                .iter()
                .filter_map(|(k, v)| Some((value_to_string(k)?, value_to_string(v)?)))
                .collect(),
            _ => BTreeMap::new(),
        }
    }

    pub fn hosts(&self) -> Arc<RwLock<Hosts>> {
        self.hosts.clone()
    }

    /// Validate and persist a key/value. The only public mutator.
    pub async fn set(&mut self, key: &str, value: &str) -> Result<()> {
        validate(key, value)?;
        self.data.insert(
            Value::String(key.to_string()),
            Value::String(value.to_string()),
        );
        yaml::save_mapping(&self.path, &self.data).await
    }

    /// YAML for `bbk config list`. Known keys are emitted with their stored value or
    /// the hardcoded default; unknown keys (read-passthrough) are stripped.
    pub fn effective_yaml(&self) -> Result<String> {
        let mut merged = Mapping::new();
        for key in KNOWN_KEYS {
            let value = self
                .data
                .get(*key)
                .cloned()
                .unwrap_or_else(|| Value::String(default_for(key).to_string()));
            merged.insert(Value::String((*key).into()), value);
        }
        let aliases = self
            .data
            .get("aliases")
            .cloned()
            .unwrap_or_else(|| Value::Mapping(Mapping::new()));
        merged.insert(Value::String("aliases".into()), aliases);
        Ok(serde_yaml::to_string(&Value::Mapping(merged))?)
    }
}

/// Whether `key` is a recognized `config.yml` setting (or `aliases`).
pub fn is_known_key(key: &str) -> bool {
    key == "aliases" || KNOWN_KEYS.contains(&key)
}

/// Hardcoded default for a known key. Empty string for keys with no documented default.
pub fn default_for(key: &str) -> &'static str {
    match key {
        "default_host" => DEFAULT_HOST,
        "git_protocol" => "https",
        "prompt" => "enabled",
        _ => "",
    }
}

fn validate(key: &str, value: &str) -> Result<()> {
    if !is_known_key(key) {
        bail!("unknown key '{key}'");
    }
    match key {
        "git_protocol" => {
            if !matches!(value, "https" | "ssh") {
                bail!("invalid value for git_protocol: '{value}' (must be 'https' or 'ssh')");
            }
        }
        "prompt" => {
            if !matches!(value, "enabled" | "disabled") {
                bail!("invalid value for prompt: '{value}' (must be 'enabled' or 'disabled')");
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn value_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => Some(String::new()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn load_from_empty_dir_succeeds() {
        let dir = TempDir::new().unwrap();
        let cfg = Config::load_from(dir.path()).await.unwrap();
        assert_eq!(cfg.get("editor"), None);
        assert_eq!(cfg.get_or_default("git_protocol"), "https");
        assert_eq!(cfg.get_or_default("default_host"), DEFAULT_HOST);
        assert_eq!(cfg.get_or_default("prompt"), "enabled");
    }

    #[tokio::test]
    async fn set_then_get_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut cfg = Config::load_from(dir.path()).await.unwrap();
        cfg.set("editor", "code -w").await.unwrap();
        let cfg2 = Config::load_from(dir.path()).await.unwrap();
        assert_eq!(cfg2.get("editor").as_deref(), Some("code -w"));
    }

    #[tokio::test]
    async fn unknown_key_rejected_on_set() {
        let dir = TempDir::new().unwrap();
        let mut cfg = Config::load_from(dir.path()).await.unwrap();
        let err = cfg.set("bogus", "value").await.unwrap_err();
        assert!(err.to_string().contains("unknown key 'bogus'"));
    }

    #[tokio::test]
    async fn invalid_git_protocol_rejected() {
        let dir = TempDir::new().unwrap();
        let mut cfg = Config::load_from(dir.path()).await.unwrap();
        let err = cfg.set("git_protocol", "carrier-pigeon").await.unwrap_err();
        assert!(err.to_string().contains("git_protocol"));
    }

    #[tokio::test]
    async fn invalid_prompt_rejected() {
        let dir = TempDir::new().unwrap();
        let mut cfg = Config::load_from(dir.path()).await.unwrap();
        assert!(cfg.set("prompt", "maybe").await.is_err());
        cfg.set("prompt", "disabled").await.unwrap();
    }

    #[tokio::test]
    async fn unknown_keys_in_file_are_preserved_on_save() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yml");
        tokio::fs::write(&path, "editor: vim\nfuture_setting: keepme\n")
            .await
            .unwrap();
        let mut cfg = Config::load_from(dir.path()).await.unwrap();
        // Sanity: unknown key is readable.
        assert_eq!(cfg.get("future_setting").as_deref(), Some("keepme"));
        // Mutate a known key, then reload — the unknown key should survive.
        cfg.set("editor", "nano").await.unwrap();
        let raw = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(
            raw.contains("future_setting"),
            "round-trip dropped unknown key: {raw}"
        );
        assert!(raw.contains("keepme"));
    }

    #[tokio::test]
    async fn effective_yaml_includes_defaults() {
        let dir = TempDir::new().unwrap();
        let cfg = Config::load_from(dir.path()).await.unwrap();
        let yaml = cfg.effective_yaml().unwrap();
        assert!(yaml.contains("git_protocol: https"));
        assert!(yaml.contains("default_host: bitbucket.org"));
        assert!(yaml.contains("prompt: enabled"));
        // Empty-default keys still appear, even if blank.
        assert!(yaml.contains("editor:"));
    }

    #[tokio::test]
    async fn effective_yaml_strips_unknown_keys() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yml");
        tokio::fs::write(&path, "editor: vim\nfuture_setting: keepme\n")
            .await
            .unwrap();
        let cfg = Config::load_from(dir.path()).await.unwrap();
        let yaml = cfg.effective_yaml().unwrap();
        assert!(!yaml.contains("future_setting"));
    }

    #[test]
    fn config_dir_honors_bb_config_dir() {
        // Save and restore environment to keep this hermetic relative to other tests.
        let prev = std::env::var_os("BB_CONFIG_DIR");
        std::env::set_var("BB_CONFIG_DIR", "/tmp/some-override-path-for-bbk");
        let dir = config_dir().unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/some-override-path-for-bbk"));
        match prev {
            Some(v) => std::env::set_var("BB_CONFIG_DIR", v),
            None => std::env::remove_var("BB_CONFIG_DIR"),
        }
    }
}
