//! `bb pr close [N]` — decline a pull request.

use clap::Args as ClapArgs;

use crate::cli::messages::print_success;
use crate::context::Context;
use crate::error::CliError;
use crate::git;

use super::display::source_branch;
use super::finder;

#[derive(ClapArgs, Debug)]
pub struct Args {
    pub number: Option<u32>,

    /// Add a comment before declining the PR.
    #[arg(short = 'c', long, value_name = "TEXT")]
    pub comment: Option<String>,

    /// Delete the source branch (local + remote) after declining.
    #[arg(short = 'd', long = "delete-branch")]
    pub delete_branch: bool,
}

pub async fn run(args: Args, ctx: &mut Context) -> Result<(), CliError> {
    let (repo, pr) = finder::find(ctx, args.number).await?;
    let client = ctx.api().await?.clone();

    if let Some(comment) = args.comment.as_deref().filter(|s| !s.is_empty()) {
        client
            .pull_requests()
            .add_comment(&repo, pr.id, comment)
            .await?;
    }
    let declined = client.pull_requests().decline(&repo, pr.id).await?;

    print_success(
        &mut ctx.io,
        &format!(
            "Declined pull request #{} ({}/{})",
            declined.id, repo.workspace, repo.slug
        ),
    )
    .map_err(io_err)?;

    if args.delete_branch {
        if let Some(branch) = source_branch(&declined).or_else(|| source_branch(&pr)) {
            let _ = tokio::process::Command::new("git")
                .args(["branch", "-D", &branch])
                .status()
                .await;
            let _ = git::push("origin", &format!(":{branch}")).await;
        }
    }

    Ok(())
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}
