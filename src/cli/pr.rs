use clap::Args;

use crate::context::Context;
use crate::error::CliError;

/// `bb pr ...` — stub. Filled by spec 06.
#[derive(Args, Debug)]
pub struct PrArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    extra: Vec<String>,
}

pub async fn run(_args: PrArgs, _ctx: &mut Context) -> Result<(), CliError> {
    Err(CliError::NotImplemented)
}
