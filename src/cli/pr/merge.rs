//! `bbk pr merge [N]` — merge a pull request.

use clap::{ArgGroup, Args as ClapArgs};

use crate::api::types::MergeInput;
use crate::cli::messages::print_success;
use crate::context::Context;
use crate::error::CliError;
use crate::git;

use super::display::source_branch;
use super::finder;

#[derive(ClapArgs, Debug)]
#[command(group = ArgGroup::new("strategy").required(false).args(["merge", "squash", "rebase"]))]
pub struct Args {
    pub number: Option<u32>,

    /// Use merge-commit strategy (default).
    #[arg(short = 'm', long)]
    pub merge: bool,

    /// Use squash strategy.
    #[arg(short = 's', long)]
    pub squash: bool,

    /// Use fast-forward strategy (Bitbucket calls this "fast-forward").
    #[arg(short = 'r', long)]
    pub rebase: bool,

    /// Delete the local + remote source branch after merge.
    #[arg(short = 'd', long = "delete-branch")]
    pub delete_branch: bool,

    /// Surface a clear error — Bitbucket has no native auto-merge.
    #[arg(long)]
    pub auto: bool,

    /// Merge commit subject.
    #[arg(short = 't', long, value_name = "TEXT")]
    pub subject: Option<String>,

    /// Merge commit body.
    #[arg(short = 'b', long, value_name = "TEXT")]
    pub body: Option<String>,

    /// Override the close-source-branch flag at merge time.
    #[arg(long)]
    pub close_source_branch: bool,
}

pub async fn run(args: Args, ctx: &mut Context) -> Result<(), CliError> {
    if args.auto {
        return Err(CliError::Flag(
            "Bitbucket does not support native auto-merge. Use Bitbucket's auto-merge UI or rerun this command once requirements are met.".into(),
        ));
    }

    let (repo, pr) = finder::find(ctx, args.number).await?;
    let client = ctx.api().await?.clone();

    let strategy = if args.squash {
        Some("squash".into())
    } else if args.rebase {
        Some("fast_forward".into())
    } else if args.merge {
        Some("merge_commit".into())
    } else {
        None
    };

    let message = match (args.subject.as_deref(), args.body.as_deref()) {
        (Some(s), Some(b)) => Some(format!("{s}\n\n{b}")),
        (Some(s), None) => Some(s.to_string()),
        (None, Some(b)) => Some(b.to_string()),
        (None, None) => None,
    };

    let input = MergeInput {
        merge_strategy: strategy,
        message,
        close_source_branch: if args.close_source_branch {
            Some(true)
        } else {
            None
        },
    };

    let merged = client.pull_requests().merge(&repo, pr.id, &input).await?;
    print_success(
        &mut ctx.io,
        &format!(
            "Merged pull request #{} ({}/{})",
            merged.id, repo.workspace, repo.slug
        ),
    )
    .map_err(io_err)?;

    if args.delete_branch {
        if let Some(branch) = source_branch(&merged).or_else(|| source_branch(&pr)) {
            // Local: best-effort.
            let _ = tokio::process::Command::new("git")
                .args(["branch", "-D", &branch])
                .status()
                .await;
            // Remote: push --delete origin <branch>.
            let _ = git::push("origin", &format!(":{branch}")).await;
        }
    }

    Ok(())
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}
