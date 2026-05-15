//! `bb pr ready [N]` — flip a draft PR to ready-for-review.

use clap::Args as ClapArgs;

use crate::api::types::UpdatePr;
use crate::cli::messages::print_success;
use crate::context::Context;
use crate::error::CliError;

use super::finder;

#[derive(ClapArgs, Debug)]
pub struct Args {
    pub number: Option<u32>,
}

pub async fn run(args: Args, ctx: &mut Context) -> Result<(), CliError> {
    let (repo, pr) = finder::find(ctx, args.number).await?;
    let client = ctx.api().await?.clone();
    let update = UpdatePr {
        draft: Some(false),
        ..Default::default()
    };
    client.pull_requests().update(&repo, pr.id, &update).await?;
    print_success(
        &mut ctx.io,
        &format!(
            "Marked pull request #{} ({}/{}) as ready for review",
            pr.id, repo.workspace, repo.slug
        ),
    )
    .map_err(|e| CliError::Other(e.into()))
}
