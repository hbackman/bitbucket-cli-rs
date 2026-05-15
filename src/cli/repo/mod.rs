//! `bbk repo ...` — view, list, clone, create, fork, set-default.

use clap::{Args, Subcommand};

use crate::context::Context;
use crate::error::CliError;

pub mod clone;
pub mod create;
pub mod display;
pub mod fork;
pub mod list;
pub mod set_default;
pub mod view;

#[derive(Args, Debug)]
pub struct RepoArgs {
    #[command(subcommand)]
    command: RepoCommand,
}

#[derive(Subcommand, Debug)]
enum RepoCommand {
    /// View a repository.
    View(view::ViewArgs),
    /// List repositories.
    List(list::ListArgs),
    /// Clone a repository.
    Clone(clone::CloneArgs),
    /// Create a repository.
    Create(create::CreateArgs),
    /// Fork a repository.
    Fork(fork::ForkArgs),
    /// Set the default repository for this directory.
    #[command(name = "set-default")]
    SetDefault(set_default::SetDefaultArgs),
}

pub async fn run(args: RepoArgs, ctx: &mut Context) -> Result<(), CliError> {
    match args.command {
        RepoCommand::View(a) => view::run(a, ctx).await,
        RepoCommand::List(a) => list::run(a, ctx).await,
        RepoCommand::Clone(a) => clone::run(a, ctx).await,
        RepoCommand::Create(a) => create::run(a, ctx).await,
        RepoCommand::Fork(a) => fork::run(a, ctx).await,
        RepoCommand::SetDefault(a) => set_default::run(a, ctx).await,
    }
}
