//! `bbk pr status` — current-branch PR, your open PRs, PRs awaiting your review.

use clap::Args as ClapArgs;
use tokio::try_join;

use crate::api::pull_request::ListOpts;
use crate::api::types::{PrState, PullRequest};
use crate::context::Context;
use crate::error::CliError;
use crate::git;

use super::display::{source_branch, state_colored};

const LIMIT_PER_SECTION: usize = 30;

#[derive(ClapArgs, Debug)]
pub struct Args {}

pub async fn run(_args: Args, ctx: &mut Context) -> Result<(), CliError> {
    let repo = ctx.base_repo().await?.clone();
    let client = ctx.api().await?.clone();

    let me = client.user().current().await?.username;
    let current_branch = git::current_branch().await.ok();

    let by_me_opts = ListOpts {
        state: Some(PrState::Open),
        query: Some(format!("author.username=\"{me}\"")),
        page_len: Some(LIMIT_PER_SECTION as u32),
        ..Default::default()
    };
    let review_opts = ListOpts {
        state: Some(PrState::Open),
        query: Some(format!("reviewers.username=\"{me}\"")),
        page_len: Some(LIMIT_PER_SECTION as u32),
        ..Default::default()
    };
    let branch_opts = current_branch.as_ref().map(|b| ListOpts {
        state: Some(PrState::Open),
        query: Some(format!("source.branch.name=\"{b}\"")),
        page_len: Some(5),
        ..Default::default()
    });

    let by_me_future = client
        .pull_requests()
        .list(&repo, by_me_opts)
        .collect(LIMIT_PER_SECTION);
    let review_future = client
        .pull_requests()
        .list(&repo, review_opts)
        .collect(LIMIT_PER_SECTION);

    let (by_me, awaiting) = try_join!(by_me_future, review_future)?;

    let current_branch_pr: Option<PullRequest> = if let Some(opts) = branch_opts {
        let prs = client.pull_requests().list(&repo, opts).collect(5).await?;
        prs.into_iter().next()
    } else {
        None
    };

    let cs = ctx.io.cs();
    writeln!(ctx.io.out(), "{}", cs.bold("Current branch")).map_err(io_err)?;
    match current_branch_pr.as_ref() {
        Some(pr) => writeln!(
            ctx.io.out(),
            "  #{}  {} [{}]",
            pr.id,
            pr.title,
            source_branch(pr).unwrap_or_default()
        )
        .map_err(io_err)?,
        None => writeln!(
            ctx.io.out(),
            "  (no open pull requests for the current branch)"
        )
        .map_err(io_err)?,
    }

    writeln!(ctx.io.out()).map_err(io_err)?;
    writeln!(ctx.io.out(), "{}", cs.bold("Created by you")).map_err(io_err)?;
    if by_me.is_empty() {
        writeln!(ctx.io.out(), "  (no open pull requests)").map_err(io_err)?;
    } else {
        for pr in &by_me {
            let state = state_colored(pr.state.as_deref().unwrap_or("OPEN"), &cs);
            writeln!(
                ctx.io.out(),
                "  #{}  {} [{}] {}",
                pr.id,
                pr.title,
                source_branch(pr).unwrap_or_default(),
                cs.gray(format!("({state})")),
            )
            .map_err(io_err)?;
        }
    }

    writeln!(ctx.io.out()).map_err(io_err)?;
    writeln!(
        ctx.io.out(),
        "{}",
        cs.bold("Requesting a code review from you")
    )
    .map_err(io_err)?;
    if awaiting.is_empty() {
        writeln!(ctx.io.out(), "  (none)").map_err(io_err)?;
    } else {
        for pr in &awaiting {
            writeln!(
                ctx.io.out(),
                "  #{}  {} [{}]",
                pr.id,
                pr.title,
                source_branch(pr).unwrap_or_default()
            )
            .map_err(io_err)?;
        }
    }

    Ok(())
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}
