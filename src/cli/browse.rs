use clap::Args;

use crate::context::Context;
use crate::error::CliError;

/// `bb browse ...` — stub. Filled by spec 07.
#[derive(Args, Debug)]
pub struct BrowseArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    extra: Vec<String>,
}

pub async fn run(_args: BrowseArgs, _ctx: &mut Context) -> Result<(), CliError> {
    Err(CliError::NotImplemented)
}
