//! `bbk auth token` — print the currently configured bearer token to stdout.

use clap::Args;

use crate::config::DEFAULT_HOST;
use crate::context::Context;
use crate::error::CliError;

#[derive(Args, Debug)]
pub struct TokenArgs {
    #[arg(long, value_name = "HOST")]
    pub hostname: Option<String>,
    #[arg(long, value_name = "USER")]
    pub user: Option<String>,
}

pub async fn run(args: TokenArgs, ctx: &mut Context) -> Result<(), CliError> {
    let host = args.hostname.as_deref().unwrap_or(DEFAULT_HOST);
    let source = ctx.auth_source().await?;
    let token = source
        .access_token(host, args.user.as_deref())
        .await
        .map_err(|e| CliError::Auth(e.to_string()))?;
    writeln!(ctx.io.out(), "{token}").map_err(|e| CliError::Other(e.into()))?;
    Ok(())
}
