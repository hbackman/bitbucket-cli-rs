//! YAML read/write helpers for `config.yml` and `hosts.yml`.
//!
//! Writes are atomic via `tempfile::NamedTempFile::persist` (write to a temp file in
//! the same directory, fsync, then rename). On Unix the resulting file is chmod 0600.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context as _, Result};
use serde_yaml::{Mapping, Value};
use tokio::fs;

/// Read a YAML mapping at `path`. A missing or empty file yields an empty mapping.
pub async fn load_mapping(path: &Path) -> Result<Mapping> {
    let bytes = match fs::read(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Mapping::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    if bytes.iter().all(|b| b.is_ascii_whitespace()) {
        return Ok(Mapping::new());
    }
    let value: Value = serde_yaml::from_slice(&bytes)
        .with_context(|| format!("parsing YAML in {}", path.display()))?;
    match value {
        Value::Mapping(m) => Ok(m),
        Value::Null => Ok(Mapping::new()),
        _ => bail!("expected a YAML mapping at {}", path.display()),
    }
}

/// Atomically write a YAML mapping to `path`, creating parent directories as needed.
pub async fn save_mapping(path: &Path, data: &Mapping) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("path {} has no parent", path.display()))?;
    fs::create_dir_all(parent)
        .await
        .with_context(|| format!("creating {}", parent.display()))?;
    let contents =
        serde_yaml::to_string(&Value::Mapping(data.clone())).context("serializing YAML")?;
    let parent: PathBuf = parent.to_path_buf();
    let path: PathBuf = path.to_path_buf();
    tokio::task::spawn_blocking(move || write_atomic(&path, &parent, contents.as_bytes()))
        .await
        .map_err(|e| anyhow!("join error while writing config: {e}"))??;
    Ok(())
}

fn write_atomic(path: &Path, parent: &Path, contents: &[u8]) -> Result<()> {
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("creating temp file in {}", parent.display()))?;
    tmp.write_all(contents)
        .with_context(|| format!("writing temp file for {}", path.display()))?;
    tmp.as_file_mut()
        .sync_all()
        .with_context(|| format!("fsyncing temp file for {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tmp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("setting 0600 on temp file for {}", path.display()))?;
    }
    tmp.persist(path)
        .map_err(|e| anyhow!("renaming temp file to {}: {}", path.display(), e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn load_missing_returns_empty() {
        let dir = TempDir::new().unwrap();
        let m = load_mapping(&dir.path().join("nope.yml")).await.unwrap();
        assert!(m.is_empty());
    }

    #[tokio::test]
    async fn save_then_load_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("c.yml");
        let mut m = Mapping::new();
        m.insert(
            Value::String("editor".into()),
            Value::String("code -w".into()),
        );
        save_mapping(&path, &m).await.unwrap();
        let loaded = load_mapping(&path).await.unwrap();
        assert_eq!(loaded, m);
    }

    #[tokio::test]
    async fn save_preserves_unknown_keys() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("c.yml");
        let mut m = Mapping::new();
        m.insert(
            Value::String("future_setting".into()),
            Value::String("xyz".into()),
        );
        m.insert(
            Value::String("nested".into()),
            Value::Sequence(vec![Value::String("a".into()), Value::String("b".into())]),
        );
        save_mapping(&path, &m).await.unwrap();
        let loaded = load_mapping(&path).await.unwrap();
        assert_eq!(loaded, m);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn save_writes_0600_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("hosts.yml");
        let mut m = Mapping::new();
        m.insert(Value::String("x".into()), Value::String("y".into()));
        save_mapping(&path, &m).await.unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");
    }

    #[tokio::test]
    async fn save_creates_missing_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sub").join("dir").join("c.yml");
        let mut m = Mapping::new();
        m.insert(Value::String("k".into()), Value::String("v".into()));
        save_mapping(&path, &m).await.unwrap();
        assert!(path.exists());
    }
}
