//! Persistent storage layer for auth records — splits each [`AuthRecord`] between
//! the OS keyring (tokens) and `hosts.yml` (metadata).
//!
//! When the keyring is unavailable or `--insecure-storage` is set, the tokens go
//! into hosts.yml too. We mirror gh's behavior here.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_yaml::{Mapping, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tokio::sync::RwLock;

use super::keyring::{is_not_found, KeyringBackend};
use super::{AuthKind, AuthRecord, KeyringBlob};
use crate::config::Hosts;

pub const FIELD_TYPE: &str = "type";
pub const FIELD_OAUTH_TOKEN: &str = "oauth_token";
pub const FIELD_REFRESH_TOKEN: &str = "refresh_token";
pub const FIELD_TOKEN_EXPIRES_AT: &str = "token_expires_at";
pub const FIELD_SCOPES: &str = "scopes";
pub const FIELD_GIT_PROTOCOL: &str = "git_protocol";

const ACTIVE_USER: &str = "active_user";

/// Write a credential. With `insecure = true`, tokens are saved into hosts.yml.
/// Otherwise the keyring gets the token blob and hosts.yml only stores metadata.
pub async fn write_user(
    hosts: &Arc<RwLock<Hosts>>,
    keyring: &Arc<dyn KeyringBackend>,
    rec: &AuthRecord,
    insecure: bool,
) -> Result<()> {
    // 1. Keyring side.
    let store_in_keyring = matches!(rec.kind, AuthKind::OAuth | AuthKind::ApiToken) && !insecure;
    if store_in_keyring {
        let blob = KeyringBlob {
            access_token: rec.access_token.clone(),
            refresh_token: rec.refresh_token.clone(),
            expires_at: Some(rec.expires_at),
            scopes: rec.scopes.clone(),
        };
        let json = serde_json::to_string(&blob)?;
        match keyring.set_password(&rec.host, &rec.user, &json) {
            Ok(_) => {}
            Err(e) => bail!(
                "writing token to keyring failed: {e}. Re-run with --insecure-storage to write to hosts.yml instead."
            ),
        }
    }

    // 2. hosts.yml side.
    let mut block = Mapping::new();
    block.insert(
        Value::String(FIELD_TYPE.into()),
        Value::String(rec.kind.as_str().into()),
    );
    block.insert(
        Value::String(FIELD_GIT_PROTOCOL.into()),
        Value::String(rec.git_protocol.clone()),
    );
    if !rec.scopes.is_empty() {
        block.insert(
            Value::String(FIELD_SCOPES.into()),
            Value::Sequence(
                rec.scopes
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect::<Vec<_>>(),
            ),
        );
    }
    if matches!(rec.kind, AuthKind::OAuth) {
        let expiry = rec
            .expires_at
            .format(&Rfc3339)
            .map_err(|e| anyhow!("formatting expires_at: {e}"))?;
        block.insert(
            Value::String(FIELD_TOKEN_EXPIRES_AT.into()),
            Value::String(expiry),
        );
    }
    if insecure {
        block.insert(
            Value::String(FIELD_OAUTH_TOKEN.into()),
            Value::String(rec.access_token.clone()),
        );
        if !rec.refresh_token.is_empty() {
            block.insert(
                Value::String(FIELD_REFRESH_TOKEN.into()),
                Value::String(rec.refresh_token.clone()),
            );
        }
    }

    let mut hosts_w = hosts.write().await;
    hosts_w
        .set_user_block(&rec.host, &rec.user, block)
        .await?;
    // First user for this host becomes active.
    if hosts_w.get(&rec.host, ACTIVE_USER).is_none() {
        hosts_w.set(&rec.host, ACTIVE_USER, &rec.user).await?;
    }
    Ok(())
}

/// Read a credential. `user = None` uses the host's `active_user`.
pub async fn read_user(
    hosts: &Arc<RwLock<Hosts>>,
    keyring: &Arc<dyn KeyringBackend>,
    host: &str,
    user: Option<&str>,
) -> Result<AuthRecord> {
    let hosts_r = hosts.read().await;
    let user = match user {
        Some(u) => u.to_string(),
        None => hosts_r
            .get(host, ACTIVE_USER)
            .ok_or_else(|| anyhow!("not logged in to {host}. Run `bb auth login`."))?,
    };

    let block = hosts_r.user_block(host, &user);
    if block.is_empty() {
        bail!("no credentials stored for {user}@{host}. Run `bb auth login`.");
    }

    let kind_str = scalar(&block, FIELD_TYPE).unwrap_or_else(|| "oauth".into());
    let kind =
        AuthKind::parse(&kind_str).ok_or_else(|| anyhow!("unknown auth kind {kind_str:?}"))?;
    let git_protocol = scalar(&block, FIELD_GIT_PROTOCOL).unwrap_or_else(|| "https".into());
    let scopes = sequence_of_strings(&block, FIELD_SCOPES);

    // Try keyring first; fall back to plaintext fields in hosts.yml.
    let kr = keyring.get_password(host, &user);
    let (access_token, refresh_token, expires_at) = match kr {
        Ok(blob) => {
            let parsed: KeyringBlob =
                serde_json::from_str(&blob).map_err(|e| anyhow!("decoding keyring blob: {e}"))?;
            let expiry = parsed
                .expires_at
                .or_else(|| parse_expiry(&block))
                .unwrap_or_else(default_expiry);
            (parsed.access_token, parsed.refresh_token, expiry)
        }
        Err(e) if is_not_found(&e) => {
            let access = scalar(&block, FIELD_OAUTH_TOKEN).unwrap_or_default();
            let refresh = scalar(&block, FIELD_REFRESH_TOKEN).unwrap_or_default();
            let expiry = parse_expiry(&block).unwrap_or_else(default_expiry);
            (access, refresh, expiry)
        }
        Err(e) => bail!("reading keyring for {user}@{host}: {e}"),
    };

    if access_token.is_empty() && matches!(kind, AuthKind::OAuth | AuthKind::ApiToken) {
        bail!("no access token stored for {user}@{host}. Run `bb auth login`.");
    }

    Ok(AuthRecord {
        host: host.to_string(),
        user,
        kind,
        access_token,
        refresh_token,
        expires_at,
        scopes,
        git_protocol,
    })
}

fn scalar(m: &Mapping, key: &str) -> Option<String> {
    match m.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn sequence_of_strings(m: &Mapping, key: &str) -> Vec<String> {
    match m.get(key) {
        Some(Value::Sequence(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_expiry(m: &Mapping) -> Option<OffsetDateTime> {
    let raw = scalar(m, FIELD_TOKEN_EXPIRES_AT)?;
    OffsetDateTime::parse(&raw, &Rfc3339).ok()
}

fn default_expiry() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::keyring::MemKeyring;
    use std::path::Path;
    use tempfile::TempDir;
    use time::macros::datetime;

    async fn fresh(dir: &Path) -> (Arc<RwLock<Hosts>>, Arc<dyn KeyringBackend>) {
        let h = Hosts::load_from(&dir.join("hosts.yml")).await.unwrap();
        let hosts = Arc::new(RwLock::new(h));
        let kr: Arc<dyn KeyringBackend> = Arc::new(MemKeyring::new());
        (hosts, kr)
    }

    fn sample(host: &str, user: &str) -> AuthRecord {
        AuthRecord {
            host: host.into(),
            user: user.into(),
            kind: AuthKind::OAuth,
            access_token: "access-1".into(),
            refresh_token: "refresh-1".into(),
            expires_at: datetime!(2026-12-31 00:00 UTC),
            scopes: vec!["account".into(), "repository".into()],
            git_protocol: "https".into(),
        }
    }

    #[tokio::test]
    async fn write_then_read_with_keyring() {
        let dir = TempDir::new().unwrap();
        let (hosts, kr) = fresh(dir.path()).await;
        write_user(&hosts, &kr, &sample("bitbucket.org", "hbackman"), false)
            .await
            .unwrap();
        let rec = read_user(&hosts, &kr, "bitbucket.org", None).await.unwrap();
        assert_eq!(rec.user, "hbackman");
        assert_eq!(rec.access_token, "access-1");
        assert_eq!(rec.refresh_token, "refresh-1");
        assert_eq!(rec.scopes, vec!["account".to_string(), "repository".into()]);
        // hosts.yml must NOT contain the token when keyring is used.
        let raw = std::fs::read_to_string(dir.path().join("hosts.yml")).unwrap();
        assert!(!raw.contains("access-1"), "tokens leaked into hosts.yml: {raw}");
    }

    #[tokio::test]
    async fn insecure_storage_writes_token_to_hosts_yml() {
        let dir = TempDir::new().unwrap();
        let (hosts, kr) = fresh(dir.path()).await;
        write_user(&hosts, &kr, &sample("bitbucket.org", "hbackman"), true)
            .await
            .unwrap();
        let raw = std::fs::read_to_string(dir.path().join("hosts.yml")).unwrap();
        assert!(raw.contains("access-1"), "expected plaintext token in hosts.yml: {raw}");
    }

    #[tokio::test]
    async fn first_user_becomes_active() {
        let dir = TempDir::new().unwrap();
        let (hosts, kr) = fresh(dir.path()).await;
        write_user(&hosts, &kr, &sample("bitbucket.org", "alice"), false)
            .await
            .unwrap();
        let h = hosts.read().await;
        assert_eq!(h.get("bitbucket.org", "active_user").as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn read_user_falls_back_to_plaintext_when_keyring_empty() {
        let dir = TempDir::new().unwrap();
        let (hosts, kr) = fresh(dir.path()).await;
        write_user(&hosts, &kr, &sample("bitbucket.org", "hbackman"), true)
            .await
            .unwrap();
        // Swap to an empty keyring so the read path is forced to hosts.yml.
        let kr2: Arc<dyn KeyringBackend> = Arc::new(MemKeyring::new());
        let rec = read_user(&hosts, &kr2, "bitbucket.org", None).await.unwrap();
        assert_eq!(rec.access_token, "access-1");
        assert_eq!(rec.refresh_token, "refresh-1");
    }
}
