//! `bb pr reopen [N]` — Bitbucket has no reopen endpoint; surface a clear error.

use clap::Args as ClapArgs;

use crate::context::Context;
use crate::error::CliError;

#[derive(ClapArgs, Debug)]
pub struct Args {
    pub number: Option<u32>,
}

pub async fn run(_args: Args, _ctx: &mut Context) -> Result<(), CliError> {
    Err(CliError::Flag(
        "Bitbucket does not support reopening declined pull requests. \
         Create a new PR from the same branch instead."
            .into(),
    ))
}
