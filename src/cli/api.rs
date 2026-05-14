use clap::Args;

use crate::context::Context;
use crate::error::CliError;

/// `bb api ...` — stub. Filled by spec 04.
#[derive(Args, Debug)]
pub struct ApiArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    extra: Vec<String>,
}

pub async fn run(_args: ApiArgs, _ctx: &mut Context) -> Result<(), CliError> {
    Err(CliError::NotImplemented)
}
