//! `bb pr checkout N` — check out a pull request's source branch locally.

use clap::Args as ClapArgs;

use crate::api::types::PullRequest;
use crate::bbrepo::BbRepo;
use crate::cli::messages::{print_notice, print_success};
use crate::context::Context;
use crate::error::CliError;
use crate::git;

use super::display::source_branch;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Pull request number.
    pub number: u32,

    /// Local branch name. Defaults to the PR's source branch.
    #[arg(short = 'b', long = "branch", value_name = "NAME")]
    pub branch: Option<String>,

    /// Reset the local branch to the remote source branch if it already exists.
    #[arg(long)]
    pub force: bool,

    /// Check out the PR in detached-HEAD mode rather than creating a branch.
    #[arg(long)]
    pub detach: bool,

    #[arg(long)]
    pub recurse_submodules: bool,
}

pub async fn run(args: Args, ctx: &mut Context) -> Result<(), CliError> {
    let repo = ctx.base_repo().await?.clone();
    let client = ctx.api().await?.clone();
    let pr = client.pull_requests().get(&repo, args.number).await?;

    let source = source_branch(&pr).ok_or_else(|| {
        CliError::Other(anyhow::anyhow!(
            "PR #{} has no source branch in API response",
            args.number
        ))
    })?;
    let remote = remote_for(&pr, &repo);
    let local = args.branch.clone().unwrap_or_else(|| source.clone());

    print_notice(&mut ctx.io, &format!("Fetching {source} from {remote}...")).map_err(io_err)?;
    git::fetch(
        &remote,
        &[&format!("{source}:refs/remotes/{remote}/{source}")],
    )
    .await
    .map_err(CliError::Other)?;

    if args.detach {
        git::checkout_branch(&format!("{remote}/{source}"))
            .await
            .map_err(CliError::Other)?;
        print_success(
            &mut ctx.io,
            &format!("Checked out {remote}/{source} (detached HEAD)"),
        )
        .map_err(io_err)?;
        return Ok(());
    }

    let local_exists = git::config_get(&format!("branch.{local}.remote"))
        .await
        .ok()
        .filter(|s| !s.is_empty())
        .is_some()
        || git::checkout_branch(&local).await.is_ok();

    if local_exists {
        // Already on it (checkout succeeded). Optionally fast-forward / reset.
        if args.force {
            // Reset to the remote tip.
            let target = format!("{remote}/{source}");
            git::fetch(&remote, &[&source])
                .await
                .map_err(CliError::Other)?;
            // `git reset --hard target`
            tokio::process::Command::new("git")
                .args(["reset", "--hard", &target])
                .status()
                .await
                .map_err(|e| CliError::Other(e.into()))?;
        } else {
            // fast-forward only.
            let _ = tokio::process::Command::new("git")
                .args(["merge", "--ff-only", &format!("{remote}/{source}")])
                .status()
                .await;
        }
    } else {
        git::create_branch_tracking(&local, &format!("{remote}/{source}"))
            .await
            .map_err(CliError::Other)?;
    }

    print_success(&mut ctx.io, &format!("Checked out branch {local}")).map_err(io_err)?;
    Ok(())
}

/// Pick the remote to fetch from. For same-repo PRs that's `origin`; for fork
/// PRs we'd want a side remote. For MVP we always use `origin` — fork handling
/// is mentioned as future work in spec 06.
fn remote_for(_pr: &PullRequest, _repo: &BbRepo) -> String {
    "origin".to_string()
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}
