//! Authentication: OAuth flow, API-token ingestion, token storage, token refresh,
//! and the git-credential helper.
//!
//! Spec: `docs/specs/02-authentication.md`.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::RwLock;

use crate::config::Hosts;

pub mod browser;
pub mod callback;
pub mod git_credential;
pub mod hosts;
pub mod keyring;
pub mod oauth;
pub mod user;

pub use browser::{Browser, DefaultBrowser, RecordingBrowser};
pub use keyring::{KeyringBackend, MemKeyring, SystemKeyring};
pub use oauth::{
    build_authorize_url, exchange_code, oauth_client, refresh_oauth_token, OAuthTokens,
};

/// Default scopes requested on `bbk auth login`. Override with `--scopes`.
pub const DEFAULT_SCOPES: &[&str] = &[
    "account",
    "repository",
    "repository:write",
    "pullrequest",
    "pullrequest:write",
    "issue",
    "webhook",
];

/// The Bitbucket "username for token auth over HTTPS." Used by the git-credential
/// helper, where the username field is ignored and the password is the token.
pub const TOKEN_AUTH_USER: &str = "x-token-auth";

/// Bitbucket-Cloud env-var overrides for the embedded OAuth client (see `build.rs`).
pub const BB_OAUTH_CLIENT_ID_ENV: &str = "BB_OAUTH_CLIENT_ID";
pub const BB_OAUTH_CLIENT_SECRET_ENV: &str = "BB_OAUTH_CLIENT_SECRET";

/// Env-var overrides for an externally-managed bearer token. Highest precedence.
pub const BB_TOKEN_ENV: &str = "BB_TOKEN";
pub const BITBUCKET_TOKEN_ENV: &str = "BITBUCKET_TOKEN";

/// Embedded client_id from `build.rs`. Empty when not configured at compile time.
pub fn embedded_client_id() -> &'static str {
    env!("BB_EMBEDDED_OAUTH_CLIENT_ID")
}

/// Embedded client_secret from `build.rs`. Empty when not configured at compile time.
pub fn embedded_client_secret() -> &'static str {
    env!("BB_EMBEDDED_OAUTH_CLIENT_SECRET")
}

/// Resolve the OAuth client id from flag → env → embedded build value.
pub fn resolve_client_id(flag: Option<&str>) -> String {
    if let Some(v) = flag.filter(|s| !s.is_empty()) {
        return v.to_string();
    }
    if let Ok(v) = std::env::var(BB_OAUTH_CLIENT_ID_ENV) {
        if !v.is_empty() {
            return v;
        }
    }
    embedded_client_id().to_string()
}

/// Resolve the OAuth client secret from flag → env → embedded build value.
pub fn resolve_client_secret(flag: Option<&str>) -> String {
    if let Some(v) = flag.filter(|s| !s.is_empty()) {
        return v.to_string();
    }
    if let Ok(v) = std::env::var(BB_OAUTH_CLIENT_SECRET_ENV) {
        if !v.is_empty() {
            return v;
        }
    }
    embedded_client_secret().to_string()
}

/// Kind of credential stored for a (host, user) pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    /// OAuth: refreshable. Token lives in keyring or, with --insecure-storage, hosts.yml.
    OAuth,
    /// User-provided API token. No refresh.
    ApiToken,
    /// $BB_TOKEN / $BITBUCKET_TOKEN. Never persisted.
    Env,
}

impl AuthKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthKind::OAuth => "oauth",
            AuthKind::ApiToken => "api_token",
            AuthKind::Env => "env",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "oauth" => Some(AuthKind::OAuth),
            "api_token" => Some(AuthKind::ApiToken),
            "env" => Some(AuthKind::Env),
            _ => None,
        }
    }
}

/// A fully-resolved credential for one (host, user).
#[derive(Debug, Clone)]
pub struct AuthRecord {
    pub host: String,
    pub user: String,
    pub kind: AuthKind,
    pub access_token: String,
    pub refresh_token: String,
    /// Unix epoch when this OAuth access token expires. Ignored for non-OAuth kinds.
    pub expires_at: OffsetDateTime,
    pub scopes: Vec<String>,
    pub git_protocol: String,
}

impl AuthRecord {
    pub fn apply_refresh(&mut self, fresh: OAuthTokens) {
        self.access_token = fresh.access_token;
        if !fresh.refresh_token.is_empty() {
            self.refresh_token = fresh.refresh_token;
        }
        self.expires_at = fresh.expires_at;
        if !fresh.scopes.is_empty() {
            self.scopes = fresh.scopes;
        }
    }
}

/// JSON blob actually written to the OS keyring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct KeyringBlob {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Bag of dependencies used by every auth path. Cheap to clone (Arc on the
/// shared hosts/keyring/http handles).
#[derive(Clone)]
pub struct AuthSource {
    pub client_id: String,
    pub client_secret: String,
    pub hosts: Arc<RwLock<Hosts>>,
    pub keyring: Arc<dyn KeyringBackend>,
    pub http: reqwest::Client,
}

impl AuthSource {
    pub fn new(
        hosts: Arc<RwLock<Hosts>>,
        keyring: Arc<dyn KeyringBackend>,
        http: reqwest::Client,
    ) -> Self {
        Self {
            client_id: resolve_client_id(None),
            client_secret: resolve_client_secret(None),
            hosts,
            keyring,
            http,
        }
    }

    pub fn with_oauth_creds(mut self, client_id: String, client_secret: String) -> Self {
        self.client_id = client_id;
        self.client_secret = client_secret;
        self
    }

    /// Return a valid access token, refreshing first if the cached OAuth token has
    /// expired (or will within 30 seconds). For api_token / env kinds, returns the
    /// stored value as-is.
    pub async fn access_token(&self, host: &str, user: Option<&str>) -> Result<String> {
        // Env override always wins.
        if let Some(tok) = env_token() {
            return Ok(tok);
        }
        let mut rec = self.load(host, user).await?;
        match rec.kind {
            AuthKind::ApiToken | AuthKind::Env => Ok(rec.access_token),
            AuthKind::OAuth => {
                let now = OffsetDateTime::now_utc();
                if now + time::Duration::seconds(30) < rec.expires_at {
                    return Ok(rec.access_token);
                }
                if rec.refresh_token.is_empty() {
                    return Err(anyhow!(
                        "OAuth token for {}@{host} expired and no refresh token is stored. Run `bbk auth login`.",
                        rec.user
                    ));
                }
                let client = oauth_client(
                    &self.client_id,
                    &self.client_secret,
                    // Refresh doesn't actually need a redirect URI to match — use a placeholder.
                    "http://localhost",
                )?;
                let fresh = refresh_oauth_token(&client, &rec.refresh_token).await?;
                rec.apply_refresh(fresh);
                self.store(&rec, /* insecure */ false).await?;
                Ok(rec.access_token.clone())
            }
        }
    }

    /// Force a refresh, ignoring the cached expiry. Called by the API client on a 401.
    pub async fn refresh_now(&self, host: &str, user: Option<&str>) -> Result<String> {
        if env_token().is_some() {
            // Env-provided token can't be refreshed.
            return Err(anyhow!(
                "{BB_TOKEN_ENV} is set but the server rejected it. Unset it or update the token."
            ));
        }
        let mut rec = self.load(host, user).await?;
        if !matches!(rec.kind, AuthKind::OAuth) {
            return Err(anyhow!(
                "credential for {}@{host} is not OAuth-based and cannot be refreshed",
                rec.user
            ));
        }
        if rec.refresh_token.is_empty() {
            return Err(anyhow!(
                "no refresh token stored for {}@{host}. Run `bbk auth login`.",
                rec.user
            ));
        }
        let client = oauth_client(&self.client_id, &self.client_secret, "http://localhost")?;
        let fresh = refresh_oauth_token(&client, &rec.refresh_token).await?;
        rec.apply_refresh(fresh);
        self.store(&rec, false).await?;
        Ok(rec.access_token)
    }

    /// Read a stored credential. `user = None` means "use active_user for the host".
    pub async fn load(&self, host: &str, user: Option<&str>) -> Result<AuthRecord> {
        hosts::read_user(&self.hosts, &self.keyring, host, user).await
    }

    /// Persist a credential. Tokens stay out of hosts.yml unless `insecure`.
    pub async fn store(&self, rec: &AuthRecord, insecure: bool) -> Result<()> {
        hosts::write_user(&self.hosts, &self.keyring, rec, insecure).await
    }

    /// Make `user` the active account for `host`. Persists immediately.
    pub async fn set_active_user(&self, host: &str, user: &str) -> Result<()> {
        let hosts = self.hosts.clone();
        let mut hosts = hosts.write().await;
        hosts.set(host, "active_user", user).await
    }

    /// Forget a credential. Wipes the keyring entry and the hosts.yml block. If the
    /// removed user was active, `active_user` is unset (caller can reassign).
    pub async fn logout(&self, host: &str, user: &str) -> Result<()> {
        let _ = self.keyring.delete_password(host, user);
        let active_was_user = {
            let h = self.hosts.read().await;
            h.get(host, "active_user").as_deref() == Some(user)
        };
        {
            let mut h = self.hosts.write().await;
            h.remove_user(host, user).await?;
            if active_was_user {
                // Promote any remaining user (deterministic: first by sort order).
                let mut users = h.users(host);
                users.sort();
                if let Some(next) = users.into_iter().next() {
                    h.set(host, "active_user", &next).await?;
                } else {
                    h.remove_host(host).await?;
                }
            }
        }
        Ok(())
    }
}

/// Highest-priority bearer token from env (BB_TOKEN > BITBUCKET_TOKEN).
pub fn env_token() -> Option<String> {
    for var in [BB_TOKEN_ENV, BITBUCKET_TOKEN_ENV] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}
