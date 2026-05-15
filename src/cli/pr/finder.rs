//! Resolve `[N]` arguments on `bbk pr` subcommands.
//!
//! Lookup order:
//! 1. Explicit numeric PR ID — fetch directly.
//! 2. No argument: find a PR whose source branch == current git branch.
//!    Multiple matches → error listing them.
//!    Zero matches → error asking for a PR number.

use crate::api::pull_request::ListOpts;
use crate::api::types::{PrState, PullRequest};
use crate::bbrepo::BbRepo;
use crate::context::Context;
use crate::error::CliError;
use crate::git;

use super::display::source_branch;

const BRANCH_SEARCH_LIMIT: usize = 50;

/// Resolve a PR by explicit id or current branch. The repo is also returned so
/// downstream code doesn't fetch it twice.
pub async fn find(ctx: &Context, id: Option<u32>) -> Result<(BbRepo, PullRequest), CliError> {
    let repo = ctx.base_repo().await?.clone();
    let client = ctx.api().await?.clone();

    if let Some(n) = id {
        let pr = client.pull_requests().get(&repo, n).await?;
        return Ok((repo, pr));
    }

    let branch = git::current_branch().await.map_err(|e| {
        CliError::Flag(format!(
            "no pull request specified — provide a PR number or run inside a git checkout: {e}"
        ))
    })?;

    let mut matches: Vec<PullRequest> = Vec::new();
    for state in [PrState::Open, PrState::Merged, PrState::Declined] {
        let opts = ListOpts {
            state: Some(state),
            query: Some(format!("source.branch.name=\"{branch}\"")),
            page_len: Some(BRANCH_SEARCH_LIMIT as u32),
            ..Default::default()
        };
        let prs = client
            .pull_requests()
            .list(&repo, opts)
            .collect(BRANCH_SEARCH_LIMIT)
            .await?;
        matches.extend(
            prs.into_iter()
                .filter(|p| source_branch(p).as_deref() == Some(branch.as_str())),
        );
        if !matches.is_empty() {
            break;
        }
    }

    match matches.len() {
        0 => Err(CliError::Flag(format!(
            "no pull request specified, and no PR found for branch {branch:?}. \
             Provide a PR number explicitly."
        ))),
        1 => Ok((repo, matches.into_iter().next().unwrap())),
        _ => {
            let listing = matches
                .iter()
                .map(|p| format!("  #{} {}", p.id, p.title))
                .collect::<Vec<_>>()
                .join("\n");
            Err(CliError::Flag(format!(
                "multiple pull requests match branch {branch:?}. Specify a PR number:\n{listing}"
            )))
        }
    }
}
