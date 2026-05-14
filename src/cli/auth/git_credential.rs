//! `bb auth git-credential <op>` — git credential helper backend.

use clap::Args;

use crate::auth::git_credential::{format_response, parse_input, CredentialOp};
use crate::auth::TOKEN_AUTH_USER;
use crate::context::Context;
use crate::error::CliError;

#[derive(Args, Debug)]
pub struct GitCredentialArgs {
    /// Operation as defined by the git-credential protocol: get | store | erase.
    pub op: String,
}

pub async fn run(args: GitCredentialArgs, ctx: &mut Context) -> Result<(), CliError> {
    let op = CredentialOp::parse(&args.op)
        .map_err(|e| CliError::Flag(e.to_string()))?;
    if !matches!(op, CredentialOp::Get) {
        // `store` and `erase` are deliberately no-ops — bb owns the credential store.
        return Ok(());
    }

    let mut stdin = String::new();
    ctx.io
        .input()
        .read_to_string(&mut stdin)
        .map_err(|e| CliError::Other(e.into()))?;
    let req = parse_input(&stdin);

    let host = req
        .get("host")
        .cloned()
        .ok_or_else(|| CliError::Flag("git-credential request missing `host`".into()))?;
    let protocol = req
        .get("protocol")
        .cloned()
        .unwrap_or_else(|| "https".to_string());

    let source = ctx.auth_source().await?;
    let token = source
        .access_token(&host, None)
        .await
        .map_err(|e| CliError::Auth(e.to_string()))?;
    let resp = format_response(&host, TOKEN_AUTH_USER, &token, &protocol);
    write!(ctx.io.out(), "{resp}").map_err(|e| CliError::Other(e.into()))?;
    Ok(())
}
