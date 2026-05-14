use clap::Args;

use crate::context::Context;
use crate::error::CliError;

/// `bb repo ...` — stub. Filled by spec 07.
#[derive(Args, Debug)]
pub struct RepoArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    extra: Vec<String>,
}

pub async fn run(_args: RepoArgs, _ctx: &mut Context) -> Result<(), CliError> {
    Err(CliError::NotImplemented)
}
