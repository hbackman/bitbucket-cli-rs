//! HTTP / API error types surfaced by the transport layer.
//!
//! `ApiError` is the structured error every API call returns. The `From` impl
//! into [`CliError`] lets command code use `?` directly when an `ApiError`
//! escapes the API layer — most CLI commands want exactly that.

use serde::Deserialize;

use crate::error::CliError;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("network: {0}")]
    Network(#[from] reqwest::Error),

    /// Authentication failed even after a refresh attempt.
    #[error("authentication failed: {hint}")]
    Auth { hint: String },

    /// Rate-limited and exhausted our retry budget.
    #[error("rate limited (retry after {retry_after_secs}s)")]
    RateLimit { retry_after_secs: u64 },

    /// HTTP status >= 400 with a parsed (or fallback) error message. Boxed so
    /// the enum stays small enough to keep `Result<T, ApiError>` lean (clippy's
    /// `result_large_err` threshold).
    #[error("{0}")]
    Response(Box<ResponseError>),
}

#[derive(Debug)]
pub struct ResponseError {
    pub status: reqwest::StatusCode,
    pub method: reqwest::Method,
    pub url: String,
    pub message: String,
    pub errors: Vec<BitbucketError>,
    pub raw: bytes::Bytes,
}

impl std::fmt::Display for ResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} {}: {}",
            self.status, self.method, self.url, self.message
        )
    }
}

/// The `error` block inside Bitbucket's error envelope.
#[derive(Debug, Clone, Deserialize)]
pub struct BitbucketError {
    pub message: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

/// Bitbucket's standard error envelope: `{"type":"error","error":{"message":...}}`.
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorEnvelope {
    #[serde(default)]
    pub error: Option<BitbucketError>,
}

impl ApiError {
    /// Parse a Bitbucket error envelope from a response body, falling back to the
    /// status line text if the body isn't JSON or doesn't match the envelope shape.
    pub fn from_response(
        method: reqwest::Method,
        url: String,
        status: reqwest::StatusCode,
        body: bytes::Bytes,
    ) -> Self {
        let (message, errors) = parse_envelope(&body)
            .unwrap_or_else(|| (status.canonical_reason().unwrap_or("error").to_string(), vec![]));
        Self::Response(Box::new(ResponseError {
            status,
            method,
            url,
            message,
            errors,
            raw: body,
        }))
    }

    pub fn is_not_found(&self) -> bool {
        matches!(self, ApiError::Response(r) if r.status == reqwest::StatusCode::NOT_FOUND)
    }
}

fn parse_envelope(body: &[u8]) -> Option<(String, Vec<BitbucketError>)> {
    let env: ErrorEnvelope = serde_json::from_slice(body).ok()?;
    let e = env.error?;
    let msg = match &e.detail {
        Some(d) if !d.is_empty() => format!("{}: {}", e.message, d),
        _ => e.message.clone(),
    };
    Some((msg, vec![e]))
}

impl From<ApiError> for CliError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::Auth { hint } => CliError::Auth(hint),
            ApiError::RateLimit { retry_after_secs } => CliError::RateLimit { retry_after_secs },
            ApiError::Response(r) if r.status == reqwest::StatusCode::NOT_FOUND => {
                CliError::NotFound(format!("{}: {}", r.url, r.message))
            }
            other => CliError::Other(anyhow::Error::from(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bitbucket_envelope() {
        let body = br#"{"type":"error","error":{"message":"Repository not found.","detail":"x"}}"#;
        let err = ApiError::from_response(
            reqwest::Method::GET,
            "https://api.bitbucket.org/2.0/repositories/x/y".into(),
            reqwest::StatusCode::NOT_FOUND,
            bytes::Bytes::from_static(body),
        );
        match err {
            ApiError::Response(r) => {
                assert!(r.message.contains("Repository not found"));
                assert_eq!(r.errors.len(), 1);
            }
            _ => panic!("expected Response"),
        }
    }

    #[test]
    fn falls_back_to_status_text_on_unparseable_body() {
        let err = ApiError::from_response(
            reqwest::Method::GET,
            "https://x".into(),
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            bytes::Bytes::from_static(b"<html>oops</html>"),
        );
        match err {
            ApiError::Response(r) => {
                assert_eq!(r.message, "Internal Server Error");
            }
            _ => panic!("expected Response"),
        }
    }
}
