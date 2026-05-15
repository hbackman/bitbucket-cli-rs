//! `GET /2.0/user` — used to validate a fresh access/api token and to learn the
//! display username that hosts.yml is keyed by.

use anyhow::{anyhow, bail, Context as _, Result};
use serde::Deserialize;

pub const USER_URL: &str = "https://api.bitbucket.org/2.0/user";

#[derive(Debug, Clone, Deserialize)]
pub struct BitbucketUser {
    /// Account name as displayed on bitbucket.org.
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    /// Atlassian account UUID.
    #[serde(default)]
    pub uuid: Option<String>,
}

/// Validate `token` and return the user that owns it.
pub async fn fetch_user(http: &reqwest::Client, token: &str) -> Result<BitbucketUser> {
    fetch_user_at(http, USER_URL, token).await
}

/// Same as [`fetch_user`] but lets the caller override the base URL (used by tests
/// against `wiremock`).
pub async fn fetch_user_at(
    http: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<BitbucketUser> {
    let resp = http
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        bail!("Bitbucket rejected the token (401). Run `bb auth login`.");
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "GET {url} returned {status}: {}",
            body.chars().take(200).collect::<String>()
        ));
    }
    let user: BitbucketUser = resp
        .json()
        .await
        .with_context(|| format!("parsing {url}"))?;
    if user.username.is_empty() {
        bail!("Bitbucket /2.0/user response had no `username` field");
    }
    Ok(user)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetch_user_ok() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/2.0/user"))
            .and(header("authorization", "Bearer hello"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"username": "hbackman"})),
            )
            .mount(&server)
            .await;
        let http = reqwest::Client::new();
        let url = format!("{}/2.0/user", server.uri());
        let user = fetch_user_at(&http, &url, "hello").await.unwrap();
        assert_eq!(user.username, "hbackman");
    }

    #[tokio::test]
    async fn fetch_user_401_returns_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/2.0/user"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let http = reqwest::Client::new();
        let url = format!("{}/2.0/user", server.uri());
        let err = fetch_user_at(&http, &url, "bad").await.unwrap_err();
        assert!(err.to_string().contains("401"));
    }
}
