//! `bb auth setup-git` — register `bb` as a git credential helper for a host.

use clap::Args;

use crate::config::DEFAULT_HOST;
use crate::context::Context;
use crate::error::CliError;
use crate::git;

#[derive(Args, Debug)]
pub struct SetupGitArgs {
    #[arg(long, value_name = "HOST")]
    pub hostname: Option<String>,
    /// Rewrite the helper even if one is already configured for this host.
    #[arg(long)]
    pub force: bool,
}

pub async fn run(args: SetupGitArgs, ctx: &mut Context) -> Result<(), CliError> {
    let host = args.hostname.as_deref().unwrap_or(DEFAULT_HOST);
    let url = format!("https://{host}");
    let exe = std::env::current_exe()
        .map_err(|e| CliError::Other(anyhow::anyhow!("locating bb executable: {e}")))?;
    let helper_cmd = format!("!{} auth git-credential", exe.display());

    let key_helper = format!("credential.{url}.helper");

    if !args.force {
        if let Ok(existing) = git::config_get_global(&key_helper).await {
            if !existing.is_empty() && existing.contains("bb auth git-credential") {
                writeln!(
                    ctx.io.err(),
                    "✓ Git already configured to use bb for {host}"
                )
                .map_err(|e| CliError::Other(e.into()))?;
                return Ok(());
            }
        }
    }

    // Clear any pre-existing helper chain, then add ours.
    git::config_unset_global_all(&key_helper)
        .await
        .map_err(CliError::Other)?;
    git::config_add_global(&key_helper, "").await.map_err(CliError::Other)?;
    git::config_add_global(&key_helper, &helper_cmd)
        .await
        .map_err(CliError::Other)?;

    writeln!(
        ctx.io.err(),
        "✓ Git operations on {host} configured to use bb as the credential helper"
    )
    .map_err(|e| CliError::Other(e.into()))?;
    Ok(())
}
