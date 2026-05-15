//! "Newer version available" notifier.
//!
//! Once per 24 hours, fetch GitHub's "latest release" for the project repo and
//! compare against the running `version`. When newer, return a `Notice` so the
//! caller can render it after the main output.
//!
//! Disabled when `BB_NO_UPDATE_NOTIFIER` or `CI` is set.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::{Context as _, Result};
use semver::Version;
use serde::{Deserialize, Serialize};

const RELEASE_API: &str = "https://api.github.com/repos/hbackman/bitbucket-cli/releases/latest";
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CacheFile {
    /// Unix timestamp of the last check.
    #[serde(default)]
    checked_at: i64,
    /// Latest tag as fetched (e.g. `v0.2.0`).
    #[serde(default)]
    latest_tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubRelease {
    tag_name: String,
}

#[derive(Debug, Clone)]
pub struct Notice {
    pub current: String,
    pub latest: String,
    pub upgrade_hint: String,
}

pub fn disabled() -> bool {
    if std::env::var_os("BB_NO_UPDATE_NOTIFIER").is_some_and(|v| v != "0") {
        return true;
    }
    if let Ok(v) = std::env::var("CI") {
        if v == "1" || v.eq_ignore_ascii_case("true") {
            return true;
        }
    }
    false
}

fn cache_path() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("BB_CACHE_DIR") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p).join("update-check.json"));
        }
    }
    if let Some(home) = std::env::var_os("XDG_CACHE_HOME") {
        if !home.is_empty() {
            return Ok(PathBuf::from(home).join("bbk").join("update-check.json"));
        }
    }
    let dirs = directories::ProjectDirs::from("", "", "bbk")
        .context("could not determine cache directory")?;
    Ok(dirs.cache_dir().join("update-check.json"))
}

async fn read_cache(path: &std::path::Path) -> Option<CacheFile> {
    let bytes = tokio::fs::read(path).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

async fn write_cache(path: &std::path::Path, cf: &CacheFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    let bytes = serde_json::to_vec(cf)?;
    tokio::fs::write(path, bytes).await?;
    Ok(())
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Check for a newer release. Returns `Ok(None)` when there's no notice to show
/// (already up to date, cache fresh, errors). Never propagates network errors.
pub async fn check(http: &reqwest::Client, current: &str) -> Result<Option<Notice>> {
    if disabled() {
        return Ok(None);
    }
    let path = match cache_path() {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    let cache = read_cache(&path).await.unwrap_or_default();
    let now = now_secs();

    let latest_tag = if (now - cache.checked_at).abs() < CACHE_TTL.as_secs() as i64
        && !cache.latest_tag.is_empty()
    {
        cache.latest_tag.clone()
    } else {
        match fetch_latest(http).await {
            Ok(tag) => {
                let _ = write_cache(
                    &path,
                    &CacheFile {
                        checked_at: now,
                        latest_tag: tag.clone(),
                    },
                )
                .await;
                tag
            }
            Err(_) => return Ok(None),
        }
    };

    let latest = parse_version(&latest_tag);
    let cur = parse_version(current);
    if let (Some(latest), Some(cur)) = (latest, cur) {
        if latest > cur {
            return Ok(Some(Notice {
                current: current.to_string(),
                latest: latest.to_string(),
                upgrade_hint: upgrade_hint(),
            }));
        }
    }
    Ok(None)
}

async fn fetch_latest(http: &reqwest::Client) -> Result<String> {
    let resp = http
        .get(RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?;
    let release: GithubRelease = resp.json().await?;
    Ok(release.tag_name)
}

fn parse_version(s: &str) -> Option<Version> {
    let s = s.trim().trim_start_matches('v');
    // strip "-dev" or build-info suffixes we can't reason about.
    let pre = s
        .split_whitespace()
        .next()
        .unwrap_or(s)
        .split(' ')
        .next()
        .unwrap_or(s);
    Version::parse(pre).ok()
}

fn upgrade_hint() -> String {
    if let Ok(exe) = std::env::current_exe() {
        let s = exe.to_string_lossy();
        if s.contains("/Cellar/") || s.contains("/homebrew/") {
            return "Run `brew upgrade hbackman/bb/bb`.".into();
        }
        if s.contains("/.cargo/bin/") || s.contains("\\.cargo\\bin\\") {
            return "Run `cargo install bbk --force`.".into();
        }
    }
    "See https://github.com/hbackman/bitbucket-cli/releases/latest for the new release.".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_strips_leading_v() {
        assert!(parse_version("v0.1.0").is_some());
        assert!(parse_version("0.1.0").is_some());
        assert!(parse_version("bogus").is_none());
    }

    #[test]
    fn disabled_when_env_set() {
        let _g = scoped_env("BB_NO_UPDATE_NOTIFIER", Some("1"));
        assert!(disabled());
    }

    pub struct ScopedEnv {
        key: &'static str,
        prev: Option<std::ffi::OsString>,
    }
    impl Drop for ScopedEnv {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }
    pub fn scoped_env(key: &'static str, value: Option<&str>) -> ScopedEnv {
        let prev = std::env::var_os(key);
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
        ScopedEnv { key, prev }
    }
}
