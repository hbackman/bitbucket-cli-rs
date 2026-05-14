use clap::Args;

use crate::context::Context;
use crate::error::CliError;

/// `bb config ...` — stub. Filled by spec 03.
#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    extra: Vec<String>,
}

pub async fn run(_args: ConfigArgs, _ctx: &mut Context) -> Result<(), CliError> {
    Err(CliError::NotImplemented)
}
