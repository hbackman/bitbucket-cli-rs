//! `bbk api --cache <duration>` support.
//!
//! Stores successful GET responses in `${XDG_CACHE_HOME}/bbk/api/<sha256>`. One
//! file per entry, no background eviction — stale entries are deleted lazily on
//! read.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Cache key derived from request shape. Including the active user means
/// different accounts hit different cache slots even for identical URLs.
pub fn key(method: &str, url: &str, accept: Option<&str>, user: Option<&str>) -> String {
    let mut h = Sha256::new();
    h.update(method.as_bytes());
    h.update(b"\n");
    h.update(url.as_bytes());
    h.update(b"\n");
    h.update(accept.unwrap_or("").as_bytes());
    h.update(b"\n");
    h.update(user.unwrap_or("").as_bytes());
    hex::encode(h.finalize())
}

pub fn cache_dir() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("BB_CACHE_DIR") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    #[cfg(unix)]
    {
        if let Some(p) = std::env::var_os("XDG_CACHE_HOME") {
            if !p.is_empty() {
                return Ok(PathBuf::from(p).join("bbk").join("api"));
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            if !home.is_empty() {
                return Ok(PathBuf::from(home).join(".cache").join("bbk").join("api"));
            }
        }
    }
    let dirs = directories::ProjectDirs::from("", "", "bbk")
        .ok_or_else(|| anyhow!("could not determine bbk cache dir"))?;
    Ok(dirs.cache_dir().join("api"))
}

/// Try to read an entry under the given TTL. Returns `Ok(None)` for cache miss
/// or expired entries (the file is removed in that case).
pub async fn read(key: &str, ttl: Duration) -> Result<Option<Entry>> {
    let dir = cache_dir()?;
    let path = dir.join(key);
    let meta = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).context("stat cache file"),
    };
    let modified = meta.modified().context("read cache mtime")?;
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    if age > ttl {
        let _ = tokio::fs::remove_file(&path).await;
        return Ok(None);
    }
    let raw = tokio::fs::read(&path).await.context("read cache file")?;
    let entry: Entry = serde_json::from_slice(&raw).context("decode cache file")?;
    Ok(Some(entry))
}

pub async fn write(key: &str, entry: &Entry) -> Result<()> {
    let dir = cache_dir()?;
    tokio::fs::create_dir_all(&dir)
        .await
        .context("mkdir cache")?;
    let path = dir.join(key);
    let tmp = path.with_extension("tmp");
    let bytes = serde_json::to_vec(entry).context("encode cache entry")?;
    tokio::fs::write(&tmp, &bytes)
        .await
        .context("write cache tmp")?;
    tokio::fs::rename(&tmp, &path)
        .await
        .context("rename cache file")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_changes_with_method() {
        let a = key("GET", "https://x/y", None, None);
        let b = key("POST", "https://x/y", None, None);
        assert_ne!(a, b);
    }

    #[test]
    fn key_changes_with_user() {
        let a = key("GET", "https://x/y", None, Some("alice"));
        let b = key("GET", "https://x/y", None, Some("bob"));
        assert_ne!(a, b);
    }
}
