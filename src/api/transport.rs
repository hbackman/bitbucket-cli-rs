//! Low-level HTTP plumbing for the Bitbucket API.
//!
//! Adds `Authorization` + `User-Agent` (idempotent), retries 429 within budget,
//! refreshes the token on 401 and retries once, parses Bitbucket error envelopes
//! into typed [`ApiError`]s, and emits request/response dumps when `BB_DEBUG` is
//! set.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, USER_AGENT};
use reqwest::{Method, Request, StatusCode};

use super::debug::{log_request, log_response, DebugMode};
use super::errors::{ApiError, ResponseError};
use crate::auth::AuthSource;

const MAX_RATE_LIMIT_RETRIES: u32 = 2;
/// Bitbucket's `Retry-After` is in seconds. Cap so a misbehaving server can't park us.
const MAX_RATE_LIMIT_SLEEP_SECS: u64 = 60;

/// Buffered HTTP response. Body is fully read; callers decode at their leisure.
#[derive(Debug, Clone)]
pub struct ApiResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub url: reqwest::Url,
    pub body: Bytes,
}

impl ApiResponse {
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, ApiError> {
        serde_json::from_slice(&self.body).map_err(|e| {
            ApiError::Response(Box::new(ResponseError {
                status: self.status,
                method: Method::GET,
                url: self.url.to_string(),
                message: format!("decoding response: {e}"),
                errors: vec![],
                raw: self.body.clone(),
            }))
        })
    }
}

/// Shared transport. Cheap to clone (Arc inside).
#[derive(Clone)]
pub struct Transport {
    pub http: reqwest::Client,
    pub auth: Arc<AuthSource>,
    pub host: String,
    pub user_agent: String,
    pub debug: DebugMode,
}

impl Transport {
    pub fn new(
        http: reqwest::Client,
        auth: Arc<AuthSource>,
        host: impl Into<String>,
        user_agent: impl Into<String>,
    ) -> Self {
        Self {
            http,
            auth,
            host: host.into(),
            user_agent: user_agent.into(),
            debug: DebugMode::from_env(),
        }
    }

    /// Send through the full pipeline. Body is fully buffered into [`ApiResponse`].
    /// Use [`Transport::send_stream`] when you need an unbuffered response (e.g. diffs).
    pub async fn send(&self, req: Request) -> Result<ApiResponse, ApiError> {
        let body_snapshot = body_as_bytes(&req);
        let mut attempt_429 = 0u32;
        let mut tried_refresh = false;

        loop {
            let cloned = clone_request(&req, body_snapshot.as_ref());
            let prepared = self.inject_headers(cloned).await?;
            log_request(self.debug, &prepared);

            let method = prepared.method().clone();
            let url = prepared.url().clone();
            let resp = self
                .http
                .execute(prepared)
                .await
                .map_err(ApiError::Network)?;
            let status = resp.status();
            let headers = resp.headers().clone();
            let body = resp.bytes().await.map_err(ApiError::Network)?;
            log_response(self.debug, &method, &url, status, &headers, &body);

            if status == StatusCode::TOO_MANY_REQUESTS {
                let wait = parse_retry_after(headers.get(reqwest::header::RETRY_AFTER));
                if attempt_429 >= MAX_RATE_LIMIT_RETRIES {
                    return Err(ApiError::RateLimit {
                        retry_after_secs: wait,
                    });
                }
                attempt_429 += 1;
                tokio::time::sleep(Duration::from_secs(wait.min(MAX_RATE_LIMIT_SLEEP_SECS))).await;
                continue;
            }

            if status == StatusCode::UNAUTHORIZED && !tried_refresh {
                tried_refresh = true;
                if self.auth.refresh_now(&self.host, None).await.is_ok() {
                    continue;
                }
                return Err(ApiError::Auth {
                    hint: "the Bitbucket API rejected the current token. Run `bbk auth login`."
                        .into(),
                });
            }
            if status == StatusCode::UNAUTHORIZED {
                return Err(ApiError::Auth {
                    hint: "the Bitbucket API rejected the refreshed token. Run `bbk auth login`."
                        .into(),
                });
            }

            if status.is_success() {
                return Ok(ApiResponse {
                    status,
                    headers,
                    url,
                    body,
                });
            }
            return Err(ApiError::from_response(
                method,
                url.to_string(),
                status,
                body,
            ));
        }
    }

    /// Send and decode the JSON body. The common path for typed endpoints.
    pub async fn send_json<T: serde::de::DeserializeOwned>(
        &self,
        req: Request,
    ) -> Result<T, ApiError> {
        self.send(req).await?.json()
    }

    /// Send and discard the body. Used for endpoints that return 200 with no payload.
    pub async fn send_void(&self, req: Request) -> Result<(), ApiError> {
        let _ = self.send(req).await?;
        Ok(())
    }

    /// Streaming send: returns the raw response without buffering. No 401-refresh
    /// retry (we'd have to replay the body, which streamed bodies can't do
    /// cheaply). Use only for endpoints whose responses are too large to buffer
    /// — `pr diff` is the only one in MVP.
    pub async fn send_stream(&self, req: Request) -> Result<reqwest::Response, ApiError> {
        let prepared = self.inject_headers(req).await?;
        log_request(self.debug, &prepared);
        let method = prepared.method().clone();
        let url = prepared.url().clone();
        let resp = self
            .http
            .execute(prepared)
            .await
            .map_err(ApiError::Network)?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let body = resp.bytes().await.map_err(ApiError::Network)?;
        Err(ApiError::from_response(
            method,
            url.to_string(),
            status,
            body,
        ))
    }

    async fn inject_headers(&self, mut req: Request) -> Result<Request, ApiError> {
        let token = self
            .auth
            .access_token(&self.host, None)
            .await
            .map_err(|e| ApiError::Auth {
                hint: e.to_string(),
            })?;

        let headers = req.headers_mut();
        if !headers.contains_key(AUTHORIZATION) {
            let value =
                HeaderValue::from_str(&format!("Bearer {token}")).map_err(|e| ApiError::Auth {
                    hint: format!("token contains invalid header bytes: {e}"),
                })?;
            headers.insert(AUTHORIZATION, value);
        }
        if !headers.contains_key(USER_AGENT) {
            if let Ok(value) = HeaderValue::from_str(&self.user_agent) {
                headers.insert(USER_AGENT, value);
            }
        }
        let accept: HeaderName = reqwest::header::ACCEPT;
        if !headers.contains_key(&accept) {
            headers.insert(accept, HeaderValue::from_static("application/json"));
        }
        Ok(req)
    }
}

/// Capture a request body for safe retry. Streaming bodies aren't replayable —
/// the callers that need retry (POST/PUT) build their bodies as `Vec<u8>`.
fn body_as_bytes(req: &Request) -> Option<Bytes> {
    req.body()
        .and_then(|b| b.as_bytes())
        .map(Bytes::copy_from_slice)
}

/// Rebuild a request for replay. `reqwest::Request` isn't `Clone`.
fn clone_request(src: &Request, body: Option<&Bytes>) -> Request {
    let mut copy = Request::new(src.method().clone(), src.url().clone());
    *copy.headers_mut() = src.headers().clone();
    if let Some(b) = body {
        *copy.body_mut() = Some(reqwest::Body::from(b.clone()));
    }
    if let Some(t) = src.timeout() {
        *copy.timeout_mut() = Some(*t);
    }
    copy
}

fn parse_retry_after(h: Option<&HeaderValue>) -> u64 {
    h.and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(1)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::auth::{AuthSource, MemKeyring};
    use crate::config::Hosts;
    use serde_yaml::{Mapping, Value};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn test_auth() -> Arc<AuthSource> {
        let hosts = Arc::new(RwLock::new(Hosts::default()));
        let kr: Arc<dyn crate::auth::KeyringBackend> = Arc::new(MemKeyring::new());
        let http = reqwest::Client::new();
        Arc::new(AuthSource::new(hosts, kr, http))
    }

    /// Build an `AuthSource` pre-seeded with an `api_token` credential. Lets
    /// transport-layer tests issue calls without touching the BB_TOKEN env var
    /// (which collides under parallel test runs). The returned `TempDir` must
    /// outlive the auth source — the seeded `Hosts` writes through it once.
    pub(crate) async fn auth_with_seeded_token(
        token: &str,
    ) -> (Arc<AuthSource>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosts.yml");
        let mut hosts = Hosts::load_from(&path).await.unwrap();
        let mut block = Mapping::new();
        block.insert(
            Value::String("type".into()),
            Value::String("api_token".into()),
        );
        block.insert(
            Value::String("oauth_token".into()),
            Value::String(token.into()),
        );
        block.insert(
            Value::String("git_protocol".into()),
            Value::String("https".into()),
        );
        hosts
            .set_user_block("bitbucket.org", "test-user", block)
            .await
            .unwrap();
        hosts
            .set("bitbucket.org", "active_user", "test-user")
            .await
            .unwrap();
        let hosts = Arc::new(RwLock::new(hosts));
        let kr: Arc<dyn crate::auth::KeyringBackend> = Arc::new(MemKeyring::new());
        let http = reqwest::Client::new();
        (Arc::new(AuthSource::new(hosts, kr, http)), dir)
    }

    #[tokio::test]
    async fn parses_retry_after_header() {
        let v = HeaderValue::from_static("3");
        assert_eq!(parse_retry_after(Some(&v)), 3);
        assert_eq!(parse_retry_after(None), 1);
        let bad = HeaderValue::from_static("not-a-number");
        assert_eq!(parse_retry_after(Some(&bad)), 1);
    }

    #[tokio::test]
    async fn injects_bearer_token_from_env() {
        let _g = scoped_env("BB_TOKEN", Some("the-token"));
        let auth = test_auth();
        let t = Transport::new(
            reqwest::Client::new(),
            auth,
            "bitbucket.org",
            "bbk-test/0.0",
        );
        let req = reqwest::Client::new()
            .get("https://example.invalid/")
            .build()
            .unwrap();
        let prepared = t.inject_headers(req).await.unwrap();
        let auth_header = prepared.headers().get(AUTHORIZATION).unwrap();
        assert_eq!(auth_header.to_str().unwrap(), "Bearer the-token");
        let ua = prepared.headers().get(USER_AGENT).unwrap();
        assert_eq!(ua.to_str().unwrap(), "bbk-test/0.0");
    }

    /// RAII guard that scopes an env-var change for the duration of a test.
    /// `set_var` / `remove_var` are global; restore on drop so parallel tests
    /// don't see each other's writes.
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
