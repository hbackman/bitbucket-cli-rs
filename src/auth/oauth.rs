//! OAuth client setup, authorize-URL building, code exchange, refresh.
//! Bitbucket Cloud supports authorization-code only (no PKCE, no device flow).

use anyhow::{anyhow, bail, Context as _, Result};
use oauth2::basic::{BasicClient, BasicTokenResponse};
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, RefreshToken,
    Scope, TokenResponse, TokenUrl,
};
use time::OffsetDateTime;
use url::Url;

pub const AUTHORIZE_URL: &str = "https://bitbucket.org/site/oauth2/authorize";
pub const TOKEN_URL: &str = "https://bitbucket.org/site/oauth2/access_token";

/// Tokens returned by the authorize/refresh endpoints, normalized for our storage.
#[derive(Debug, Clone)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: OffsetDateTime,
    pub scopes: Vec<String>,
}

/// Build a Bitbucket OAuth client. Returns an error if `client_id` is empty —
/// release builds bake one in via `build.rs`; otherwise the user must pass
/// `--client-id`.
pub fn oauth_client(
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> Result<BasicClient> {
    if client_id.is_empty() {
        bail!(
            "OAuth client_id is empty. Pass --client-id or set BB_OAUTH_CLIENT_ID. \
             Release builds embed a default; you appear to be running a dev build."
        );
    }
    let mut client = BasicClient::new(
        ClientId::new(client_id.to_string()),
        if client_secret.is_empty() {
            None
        } else {
            Some(ClientSecret::new(client_secret.to_string()))
        },
        AuthUrl::new(AUTHORIZE_URL.to_string()).context("parsing authorize URL")?,
        Some(TokenUrl::new(TOKEN_URL.to_string()).context("parsing token URL")?),
    );
    client = client.set_redirect_uri(
        RedirectUrl::new(redirect_uri.to_string()).context("parsing redirect URL")?,
    );
    Ok(client)
}

/// Build the authorize URL and return it along with the CSRF state value.
/// The caller must compare the state token returned in the callback.
pub fn build_authorize_url(
    client: &BasicClient,
    scopes: &[String],
    state: CsrfToken,
) -> (Url, CsrfToken) {
    let state_clone = state.clone();
    let (url, csrf) = client
        .authorize_url(move || state_clone.clone())
        .add_scopes(scopes.iter().cloned().map(Scope::new))
        .url();
    let _ = state;
    (url, csrf)
}

/// Exchange an authorization code for an access + refresh token.
pub async fn exchange_code(client: &BasicClient, code: String) -> Result<OAuthTokens> {
    let resp = client
        .exchange_code(AuthorizationCode::new(code))
        .request_async(async_http_client)
        .await
        .map_err(|e| anyhow!("exchanging authorization code: {e}"))?;
    Ok(normalize(resp))
}

/// Exchange a refresh token for a fresh access token.
pub async fn refresh_oauth_token(client: &BasicClient, refresh_token: &str) -> Result<OAuthTokens> {
    let resp = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
        .request_async(async_http_client)
        .await
        .map_err(|e| anyhow!("refreshing OAuth token: {e}"))?;
    Ok(normalize(resp))
}

fn normalize(resp: BasicTokenResponse) -> OAuthTokens {
    let now = OffsetDateTime::now_utc();
    let expires_at = match resp.expires_in() {
        Some(d) => now + time::Duration::seconds(d.as_secs() as i64),
        None => now + time::Duration::hours(2), // Bitbucket default
    };
    let scopes = resp
        .scopes()
        .map(|s| s.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();
    let refresh_token = resp
        .refresh_token()
        .map(|t| t.secret().clone())
        .unwrap_or_default();
    OAuthTokens {
        access_token: resp.access_token().secret().clone(),
        refresh_token,
        expires_at,
        scopes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_client() -> BasicClient {
        oauth_client(
            "test-client-id",
            "test-client-secret",
            "http://localhost:54321",
        )
        .unwrap()
    }

    #[test]
    fn authorize_url_has_required_query_params() {
        let client = make_client();
        let scopes = vec!["account".to_string(), "repository".to_string()];
        let state = CsrfToken::new("the-state".to_string());
        let (url, _csrf) = build_authorize_url(&client, &scopes, state);
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(
            params.get("client_id").map(String::as_str),
            Some("test-client-id")
        );
        assert_eq!(
            params.get("response_type").map(String::as_str),
            Some("code")
        );
        assert_eq!(params.get("state").map(String::as_str), Some("the-state"));
        let scope = params.get("scope").expect("scope param");
        assert!(scope.contains("account"));
        assert!(scope.contains("repository"));
        assert_eq!(
            params.get("redirect_uri").map(String::as_str),
            Some("http://localhost:54321")
        );
    }

    #[test]
    fn empty_client_id_is_rejected() {
        let err = oauth_client("", "", "http://localhost").unwrap_err();
        assert!(err.to_string().contains("client_id"));
    }
}
