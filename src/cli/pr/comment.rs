//! `bb pr comment [N]` — add, edit, or delete a comment on a PR.

use clap::{ArgGroup, Args as ClapArgs};
use tokio::io::AsyncReadExt;

use crate::api::types::Comment;
use crate::bbrepo::BbRepo;
use crate::cli::messages::print_success;
use crate::context::Context;
use crate::error::CliError;

use super::finder;

#[derive(ClapArgs, Debug)]
#[command(group = ArgGroup::new("action").required(false).args(["edit_last", "delete_last"]))]
pub struct Args {
    pub number: Option<u32>,

    #[arg(short = 'b', long, value_name = "TEXT")]
    pub body: Option<String>,

    /// Read body from a file (`-` for stdin).
    #[arg(short = 'F', long = "body-file", value_name = "PATH")]
    pub body_file: Option<String>,

    /// Replace your most recent comment on this PR (instead of adding a new one).
    #[arg(long = "edit-last")]
    pub edit_last: bool,

    /// Add the comment if you don't already have one (pairs with `--edit-last`).
    #[arg(long = "create-if-none", requires = "edit_last")]
    pub create_if_none: bool,

    /// Delete your most recent comment on this PR.
    #[arg(long = "delete-last")]
    pub delete_last: bool,
}

pub async fn run(args: Args, ctx: &mut Context) -> Result<(), CliError> {
    let (repo, pr) = finder::find(ctx, args.number).await?;
    let client = ctx.api().await?.clone();

    if args.delete_last {
        let last = locate_last_self_comment(ctx, &client, &repo, pr.id).await?;
        let Some(c) = last else {
            return Err(CliError::Flag(
                "you have no comments on this pull request to delete".into(),
            ));
        };
        client
            .pull_requests()
            .delete_comment(&repo, pr.id, c.id)
            .await?;
        print_success(&mut ctx.io, &format!("Deleted comment #{}", c.id)).map_err(io_err)?;
        return Ok(());
    }

    let body = read_body(&args).await?;
    let body = match body {
        Some(b) if !b.trim().is_empty() => b,
        _ => return Err(CliError::Flag("comment body cannot be empty".into())),
    };

    if args.edit_last {
        let last = locate_last_self_comment(ctx, &client, &repo, pr.id).await?;
        match last {
            Some(c) => {
                client
                    .pull_requests()
                    .edit_comment(&repo, pr.id, c.id, &body)
                    .await?;
                print_success(&mut ctx.io, &format!("Updated comment #{}", c.id))
                    .map_err(io_err)?;
                return Ok(());
            }
            None if args.create_if_none => { /* fall through to create */ }
            None => {
                return Err(CliError::Flag(
                    "you have no comments on this pull request to edit. Pass --create-if-none to add one.".into(),
                ));
            }
        }
    }

    let created = client
        .pull_requests()
        .add_comment(&repo, pr.id, &body)
        .await?;
    print_success(
        &mut ctx.io,
        &format!(
            "Commented on pull request #{} ({}/{}) (comment #{})",
            pr.id, repo.workspace, repo.slug, created.id
        ),
    )
    .map_err(io_err)
}

async fn locate_last_self_comment(
    ctx: &Context,
    client: &crate::api::Client,
    repo: &BbRepo,
    pr_id: u32,
) -> Result<Option<Comment>, CliError> {
    let me = ctx.api().await?.user().current().await?.username;
    let comments: Vec<Comment> = client
        .pull_requests()
        .comments(repo, pr_id)
        .collect(0)
        .await?;
    Ok(comments
        .into_iter()
        .filter(|c| c.user.as_ref().and_then(|u| u.username.as_deref()) == Some(me.as_str()))
        .max_by(|a, b| a.created_on.cmp(&b.created_on)))
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
