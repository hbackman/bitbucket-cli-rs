//! `bbk pr ...` — pull request commands.

use clap::{Args, Subcommand};

use crate::context::Context;
use crate::error::CliError;

pub mod checkout;
pub mod checks;
pub mod close;
pub mod comment;
pub mod create;
pub mod diff;
pub mod display;
pub mod edit;
pub mod finder;
pub mod list;
pub mod markdown;
pub mod merge;
pub mod ready;
pub mod reopen;
pub mod review;
pub mod status;
pub mod view;

#[derive(Args, Debug)]
pub struct PrArgs {
    #[command(subcommand)]
    command: PrCommand,
}

#[derive(Subcommand, Debug)]
enum PrCommand {
    /// List pull requests in a repository.
    List(list::Args),
    /// View a pull request.
    View(view::Args),
    /// Show status of pull requests relevant to you.
    Status(status::Args),
    /// Create a new pull request.
    Create(create::Args),
    /// Check out a pull request's branch locally.
    Checkout(checkout::Args),
    /// Print the diff for a pull request.
    Diff(diff::Args),
    /// Merge a pull request.
    Merge(merge::Args),
    /// Close (decline) a pull request without merging.
    Close(close::Args),
    /// Reopen a declined pull request (Bitbucket does not support this).
    Reopen(reopen::Args),
    /// Add a comment to a pull request.
    Comment(comment::Args),
    /// Approve, request changes, or comment on a pull request.
    Review(review::Args),
    /// Show CI/build statuses for a pull request.
    Checks(checks::Args),
    /// Mark a draft pull request as ready for review.
    Ready(ready::Args),
    /// Edit a pull request's title, body, base, or reviewers.
    Edit(edit::Args),
}

pub async fn run(args: PrArgs, ctx: &mut Context) -> Result<(), CliError> {
    match args.command {
        PrCommand::List(a) => list::run(a, ctx).await,
        PrCommand::View(a) => view::run(a, ctx).await,
        PrCommand::Status(a) => status::run(a, ctx).await,
        PrCommand::Create(a) => create::run(a, ctx).await,
        PrCommand::Checkout(a) => checkout::run(a, ctx).await,
        PrCommand::Diff(a) => diff::run(a, ctx).await,
        PrCommand::Merge(a) => merge::run(a, ctx).await,
        PrCommand::Close(a) => close::run(a, ctx).await,
        PrCommand::Reopen(a) => reopen::run(a, ctx).await,
        PrCommand::Comment(a) => comment::run(a, ctx).await,
        PrCommand::Review(a) => review::run(a, ctx).await,
        PrCommand::Checks(a) => checks::run(a, ctx).await,
        PrCommand::Ready(a) => ready::run(a, ctx).await,
        PrCommand::Edit(a) => edit::run(a, ctx).await,
    }
}
