//! `bbk auth login`. Runs the OAuth browser flow (default) or accepts a paste-in API
//! token (`--with-token`).

use std::time::Duration;

use clap::Args;
use oauth2::CsrfToken;
use time::OffsetDateTime;

use crate::auth::{
    self, callback, oauth, resolve_client_id, resolve_client_secret, AuthKind, AuthRecord,
    AuthSource, DEFAULT_SCOPES,
};
use crate::config::DEFAULT_HOST;
use crate::context::Context;
use crate::error::CliError;

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Args, Debug)]
pub struct LoginArgs {
    /// Bitbucket hostname. Defaults to bitbucket.org.
    #[arg(long, value_name = "HOST")]
    pub hostname: Option<String>,

    /// Use the browser OAuth flow (default).
    #[arg(long, conflicts_with = "with_token")]
    pub web: bool,

    /// Read an API token from stdin and store it (no browser).
    #[arg(long = "with-token")]
    pub with_token: bool,

    /// Comma-separated OAuth scopes. Defaults to the bbk-recommended set.
    #[arg(long, value_name = "SCOPES")]
    pub scopes: Option<String>,

    /// Override the embedded OAuth client_id.
    #[arg(long, value_name = "ID")]
    pub client_id: Option<String>,

    /// Override the embedded OAuth client_secret.
    #[arg(long, value_name = "SECRET")]
    pub client_secret: Option<String>,

    /// Store the token in the OS keyring instead of plaintext in hosts.yml.
    /// Default is plaintext in hosts.yml (chmod 0600) — matches gh's UX and
    /// avoids macOS Keychain prompts on every binary rebuild for unsigned
    /// builds.
    #[arg(long)]
    pub keyring: bool,

    /// Default git protocol stored alongside the credential (https or ssh).
    #[arg(long, value_name = "PROTO", default_value = "https")]
    pub git_protocol: String,

    /// Skip the `setup-git` follow-up.
    #[arg(long)]
    pub no_setup_git: bool,
}

pub async fn run(args: LoginArgs, ctx: &mut Context) -> Result<(), CliError> {
    let hostname = args
        .hostname
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_HOST.to_string());

    if !matches!(args.git_protocol.as_str(), "https" | "ssh") {
        return Err(CliError::Flag(format!(
            "invalid --git-protocol '{}' (must be 'https' or 'ssh')",
            args.git_protocol
        )));
    }

    let source = ctx.auth_source().await?;

    let rec = if args.with_token {
        login_with_token(ctx, &source, &hostname, &args).await?
    } else {
        login_with_browser(ctx, &source, &hostname, &args).await?
    };

    let insecure = !args.keyring;
    source
        .store(&rec, insecure)
        .await
        .map_err(CliError::Other)?;
    source
        .set_active_user(&rec.host, &rec.user)
        .await
        .map_err(CliError::Other)?;

    let storage_note = if insecure { "plaintext" } else { "keyring" };
    writeln!(
        ctx.io.err(),
        "✓ Logged in to {} as {} ({})",
        rec.host,
        rec.user,
        storage_note
    )
    .map_err(|e| CliError::Other(e.into()))?;
    writeln!(
        ctx.io.err(),
        "✓ Configured git protocol {}",
        rec.git_protocol
    )
    .map_err(|e| CliError::Other(e.into()))?;

    if !args.no_setup_git {
        writeln!(
            ctx.io.err(),
            "  Run `bbk auth setup-git` to use bbk as your git credential helper for {}.",
            rec.host
        )
        .map_err(|e| CliError::Other(e.into()))?;
    }
    Ok(())
}

async fn login_with_browser(
    ctx: &mut Context,
    source: &AuthSource,
    hostname: &str,
    args: &LoginArgs,
) -> Result<AuthRecord, CliError> {
    let client_id = resolve_client_id(args.client_id.as_deref());
    let client_secret = resolve_client_secret(args.client_secret.as_deref());
    if client_id.is_empty() {
        return Err(CliError::Auth(
            "no OAuth client_id available. Pass --client-id or set BB_OAUTH_CLIENT_ID.".into(),
        ));
    }
    let scopes: Vec<String> = parse_scopes(args.scopes.as_deref());

    let (listener, port) = callback::bind_loopback().await.map_err(CliError::Other)?;
    let redirect_uri = format!("http://localhost:{port}");
    let client =
        oauth::oauth_client(&client_id, &client_secret, &redirect_uri).map_err(CliError::Other)?;
    let state = CsrfToken::new_random();
    let (auth_url, _csrf) = oauth::build_authorize_url(&client, &scopes, state.clone());

    writeln!(ctx.io.err(), "- Opening {} in your browser.", auth_url)
        .map_err(|e| CliError::Other(e.into()))?;
    let browser = ctx.browser();
    if let Err(e) = browser.open(auth_url.as_str()) {
        writeln!(
            ctx.io.err(),
            "  Could not open browser ({e}). Visit the URL above manually."
        )
        .map_err(|e| CliError::Other(e.into()))?;
    }

    let code = callback::await_callback(listener, state.secret(), CALLBACK_TIMEOUT)
        .await
        .map_err(|e| CliError::Auth(e.to_string()))?;
    let tokens = oauth::exchange_code(&client, code)
        .await
        .map_err(|e| CliError::Auth(e.to_string()))?;

    let http = ctx.http_client().clone();
    let user = auth::user::fetch_user(&http, &tokens.access_token)
        .await
        .map_err(|e| CliError::Auth(e.to_string()))?;

    let _ = source; // currently unused — store happens in run() after we return.

    let scopes_for_storage = if tokens.scopes.is_empty() {
        scopes
    } else {
        tokens.scopes.clone()
    };

    Ok(AuthRecord {
        host: hostname.to_string(),
        user: user.username,
        kind: AuthKind::OAuth,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at: tokens.expires_at,
        scopes: scopes_for_storage,
        git_protocol: args.git_protocol.clone(),
    })
}

async fn login_with_token(
    ctx: &mut Context,
    _source: &AuthSource,
    hostname: &str,
    args: &LoginArgs,
) -> Result<AuthRecord, CliError> {
    let mut buf = String::new();
    ctx.io
        .input()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::Other(e.into()))?;
    let token = buf.trim().to_string();
    if token.is_empty() {
        return Err(CliError::Flag(
            "no token on stdin. Pipe one in: `bbk auth login --with-token < token.txt`.".into(),
        ));
    }

    let http = ctx.http_client().clone();
    let user = auth::user::fetch_user(&http, &token)
        .await
        .map_err(|e| CliError::Auth(e.to_string()))?;

    Ok(AuthRecord {
        host: hostname.to_string(),
        user: user.username,
        kind: AuthKind::ApiToken,
        access_token: token,
        refresh_token: String::new(),
        expires_at: OffsetDateTime::UNIX_EPOCH,
        scopes: Vec::new(),
        git_protocol: args.git_protocol.clone(),
    })
}

fn parse_scopes(raw: Option<&str>) -> Vec<String> {
    match raw {
        Some(s) if !s.is_empty() => s
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        _ => DEFAULT_SCOPES.iter().map(|s| s.to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scopes_uses_defaults_when_empty() {
        let s = parse_scopes(None);
        assert!(s.contains(&"account".to_string()));
        assert!(s.contains(&"pullrequest".to_string()));
    }

    #[test]
    fn parse_scopes_splits_on_comma() {
        let s = parse_scopes(Some("account, repository"));
        assert_eq!(s, vec!["account".to_string(), "repository".into()]);
    }
}
