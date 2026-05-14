use clap::Args;

use crate::context::Context;
use crate::error::CliError;

/// `bb auth ...` — stub. Filled by spec 02.
#[derive(Args, Debug)]
pub struct AuthArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    extra: Vec<String>,
}

pub async fn run(_args: AuthArgs, _ctx: &mut Context) -> Result<(), CliError> {
    Err(CliError::NotImplemented)
}
