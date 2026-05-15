//! `bbk auth switch` — change the active account on a host.

use clap::Args;

use crate::config::DEFAULT_HOST;
use crate::context::Context;
use crate::error::CliError;

#[derive(Args, Debug)]
pub struct SwitchArgs {
    #[arg(long, value_name = "HOST")]
    pub hostname: Option<String>,
    /// Target user. If omitted, picks the only other user (errors when ambiguous —
    /// no interactive prompt UI yet).
    #[arg(long, short = 'u', value_name = "USER")]
    pub user: Option<String>,
}

pub async fn run(args: SwitchArgs, ctx: &mut Context) -> Result<(), CliError> {
    let host = args.hostname.as_deref().unwrap_or(DEFAULT_HOST);
    let source = ctx.auth_source().await?;

    let target = if let Some(u) = args.user {
        u
    } else {
        let h = source.hosts.read().await;
        let users = h.users(host);
        let active = h.get(host, "active_user");
        let candidates: Vec<_> = users
            .into_iter()
            .filter(|u| Some(u) != active.as_ref())
            .collect();
        match candidates.as_slice() {
            [] => {
                return Err(CliError::Auth(format!(
                    "no other accounts to switch to on {host}. Run `bbk auth login` to add one."
                )))
            }
            [only] => only.clone(),
            many => {
                return Err(CliError::Flag(format!(
                    "multiple accounts available on {host}: {}. Pass --user.",
                    many.join(", ")
                )))
            }
        }
    };

    source
        .set_active_user(host, &target)
        .await
        .map_err(CliError::Other)?;
    writeln!(
        ctx.io.err(),
        "✓ Switched active account to {target} on {host}"
    )
    .map_err(|e| CliError::Other(e.into()))?;
    Ok(())
}
