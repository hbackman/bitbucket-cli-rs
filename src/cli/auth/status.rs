//! `bb auth status` — show which hosts/users are logged in.

use clap::Args;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::auth::{env_token, AuthKind, AuthSource};
use crate::context::Context;
use crate::error::CliError;

#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Filter to a single host.
    #[arg(long, value_name = "HOST")]
    pub hostname: Option<String>,
    /// Print the bearer token after the status block.
    #[arg(long)]
    pub show_token: bool,
}

pub async fn run(args: StatusArgs, ctx: &mut Context) -> Result<(), CliError> {
    let source = ctx.auth_source().await?;
    let hosts_list: Vec<String> = {
        let h = source.hosts.read().await;
        let all = h.hosts();
        if let Some(filter) = args.hostname.as_deref() {
            all.into_iter().filter(|name| name == filter).collect()
        } else {
            all
        }
    };

    if hosts_list.is_empty() {
        return Err(CliError::Auth(
            "you are not logged in to any Bitbucket hosts. Run `bb auth login`.".into(),
        ));
    }

    for host in &hosts_list {
        print_host(ctx, &source, host, args.show_token).await?;
    }
    Ok(())
}

async fn print_host(
    ctx: &mut Context,
    source: &AuthSource,
    host: &str,
    show_token: bool,
) -> Result<(), CliError> {
    let active_user = {
        let h = source.hosts.read().await;
        h.get(host, "active_user")
    };
    writeln!(ctx.io.out(), "{host}").map_err(|e| CliError::Other(e.into()))?;
    let Some(user) = active_user else {
        writeln!(ctx.io.out(), "  - No active user").map_err(|e| CliError::Other(e.into()))?;
        return Ok(());
    };

    let storage = if env_token().is_some() {
        "from environment"
    } else {
        // If the user block has a plaintext oauth_token field, --insecure-storage was used.
        let h = source.hosts.read().await;
        let block = h.user_block(host, &user);
        if block.contains_key("oauth_token") {
            "plaintext"
        } else {
            "keyring"
        }
    };
    writeln!(
        ctx.io.out(),
        "  ✓ Logged in to {host} as {user} ({storage})"
    )
    .map_err(|e| CliError::Other(e.into()))?;
    writeln!(ctx.io.out(), "  - Active user: {user}").map_err(|e| CliError::Other(e.into()))?;

    let rec = source.load(host, None).await.map_err(CliError::Other)?;
    writeln!(
        ctx.io.out(),
        "  - Git operations protocol: {}",
        rec.git_protocol
    )
    .map_err(|e| CliError::Other(e.into()))?;
    if !rec.scopes.is_empty() {
        let joined = rec
            .scopes
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(ctx.io.out(), "  - Token scopes: {joined}")
            .map_err(|e| CliError::Other(e.into()))?;
    }
    if matches!(rec.kind, AuthKind::OAuth) {
        let now = OffsetDateTime::now_utc();
        let expires_at_str = rec.expires_at.format(&Rfc3339).unwrap_or_default();
        let delta = rec.expires_at - now;
        let human = if delta.is_negative() {
            "expired".to_string()
        } else {
            format!(
                "in {}h {}m",
                delta.whole_hours().max(0),
                (delta.whole_minutes() - delta.whole_hours() * 60).max(0)
            )
        };
        writeln!(
            ctx.io.out(),
            "  - Token expires: {expires_at_str} ({human})"
        )
        .map_err(|e| CliError::Other(e.into()))?;
    }
    if show_token {
        let token = source
            .access_token(host, None)
            .await
            .map_err(|e| CliError::Auth(e.to_string()))?;
        writeln!(ctx.io.out(), "  - Token: {token}").map_err(|e| CliError::Other(e.into()))?;
    }
    Ok(())
}
