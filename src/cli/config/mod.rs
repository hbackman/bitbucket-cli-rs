use clap::{Args, Subcommand};

use crate::context::Context;
use crate::error::CliError;

mod get;
mod list;
mod set;

/// `bb config ...` — read and modify configuration.
#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    /// Print the value of a config key.
    Get(get::GetArgs),
    /// Set the value of a config key.
    Set(set::SetArgs),
    /// Print a list of configuration keys and values.
    List(list::ListArgs),
}

pub async fn run(args: ConfigArgs, ctx: &mut Context) -> Result<(), CliError> {
    match args.command {
        ConfigCommand::Get(a) => get::run(a, ctx).await,
        ConfigCommand::Set(a) => set::run(a, ctx).await,
        ConfigCommand::List(a) => list::run(a, ctx).await,
    }
}
