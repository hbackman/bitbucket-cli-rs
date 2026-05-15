//! `bb auth logout` — drop a stored credential.

use clap::Args;

use crate::config::DEFAULT_HOST;
use crate::context::Context;
use crate::error::CliError;

#[derive(Args, Debug)]
pub struct LogoutArgs {
    /// Bitbucket hostname.
    #[arg(long, value_name = "HOST")]
    pub hostname: Option<String>,
    /// Bitbucket username. Defaults to the active user.
    #[arg(long, value_name = "USER")]
    pub user: Option<String>,
    /// Skip confirmation prompt (always skipped for now — no prompt UI yet).
    #[arg(short = 'y', long)]
    pub yes: bool,
}

pub async fn run(args: LogoutArgs, ctx: &mut Context) -> Result<(), CliError> {
    let host = args.hostname.as_deref().unwrap_or(DEFAULT_HOST).to_string();
    let source = ctx.auth_source().await?;
    let user = match args.user {
        Some(u) => u,
        None => {
            let h = source.hosts.read().await;
            h.get(&host, "active_user")
                .ok_or_else(|| CliError::Auth(format!("not logged in to {host}")))?
        }
    };
    source.logout(&host, &user).await.map_err(CliError::Other)?;
    writeln!(ctx.io.err(), "✓ Logged out of {host} as {user}")
        .map_err(|e| CliError::Other(e.into()))?;
    Ok(())
}
