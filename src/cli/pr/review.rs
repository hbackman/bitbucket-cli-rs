//! `bbk pr review [N]` — approve, request changes, or comment on a PR.

use clap::{ArgGroup, Args as ClapArgs};
use tokio::io::AsyncReadExt;

use crate::cli::messages::print_success;
use crate::context::Context;
use crate::error::CliError;

use super::finder;

#[derive(ClapArgs, Debug)]
#[command(group = ArgGroup::new("verdict")
    .required(true)
    .args([
        "approve",
        "request_changes",
        "comment",
        "undo_approve",
        "undo_request_changes",
    ]))]
pub struct Args {
    pub number: Option<u32>,

    #[arg(short = 'a', long)]
    pub approve: bool,

    #[arg(short = 'r', long = "request-changes")]
    pub request_changes: bool,

    #[arg(short = 'c', long)]
    pub comment: bool,

    #[arg(long = "undo-approve")]
    pub undo_approve: bool,

    #[arg(long = "undo-request-changes")]
    pub undo_request_changes: bool,

    #[arg(short = 'b', long, value_name = "TEXT")]
    pub body: Option<String>,

    /// Read body from a file (`-` for stdin).
    #[arg(short = 'F', long = "body-file", value_name = "PATH")]
    pub body_file: Option<String>,
}

pub async fn run(args: Args, ctx: &mut Context) -> Result<(), CliError> {
    let (repo, pr) = finder::find(ctx, args.number).await?;
    let client = ctx.api().await?.clone();
    let svc = client.pull_requests();

    let body = read_body(&args).await?;
    if args.comment {
        let body =
            body.ok_or_else(|| CliError::Flag("--comment requires --body or --body-file".into()))?;
        svc.add_comment(&repo, pr.id, &body).await?;
        print_success(
            &mut ctx.io,
            &format!("Commented on pull request #{}", pr.id),
        )
        .map_err(io_err)?;
        return Ok(());
    }

    // For approve / request-changes: optionally post a comment first.
    if let Some(body) = body.as_deref().filter(|s| !s.trim().is_empty()) {
        svc.add_comment(&repo, pr.id, body).await?;
    }

    if args.approve {
        svc.approve(&repo, pr.id).await?;
        print_success(&mut ctx.io, &format!("Approved pull request #{}", pr.id)).map_err(io_err)?;
    } else if args.undo_approve {
        svc.unapprove(&repo, pr.id).await?;
        print_success(
            &mut ctx.io,
            &format!("Removed approval from pull request #{}", pr.id),
        )
        .map_err(io_err)?;
    } else if args.request_changes {
        svc.request_changes(&repo, pr.id).await?;
        print_success(
            &mut ctx.io,
            &format!("Requested changes on pull request #{}", pr.id),
        )
        .map_err(io_err)?;
    } else if args.undo_request_changes {
        svc.unrequest_changes(&repo, pr.id).await?;
        print_success(
            &mut ctx.io,
            &format!("Cleared change-request from pull request #{}", pr.id),
        )
        .map_err(io_err)?;
    }
    Ok(())
}

async fn read_body(args: &Args) -> Result<Option<String>, CliError> {
    if let Some(b) = args.body.clone() {
        return Ok(Some(b));
    }
    let Some(path) = args.body_file.as_deref() else {
        return Ok(None);
    };
    if path == "-" {
        let mut s = String::new();
        tokio::io::stdin()
            .read_to_string(&mut s)
            .await
            .map_err(|e| CliError::Other(e.into()))?;
        return Ok(Some(s));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| CliError::Flag(format!("could not read --body-file {path}: {e}")))?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}
