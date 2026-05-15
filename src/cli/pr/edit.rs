//! `bbk pr edit [N]` — update title/body/base/reviewers on an existing PR.

use clap::Args as ClapArgs;
use tokio::io::AsyncReadExt;

use crate::api::types::{Actor, BranchInput, PrEndpointInput, ReviewerInput, UpdatePr};
use crate::cli::messages::print_success;
use crate::context::Context;
use crate::error::CliError;

use super::finder;

#[derive(ClapArgs, Debug)]
pub struct Args {
    pub number: Option<u32>,

    #[arg(short = 't', long, value_name = "TEXT")]
    pub title: Option<String>,

    #[arg(short = 'b', long, value_name = "TEXT")]
    pub body: Option<String>,

    /// Read body from a file (`-` for stdin).
    #[arg(short = 'F', long = "body-file", value_name = "PATH")]
    pub body_file: Option<String>,

    /// Destination branch.
    #[arg(short = 'B', long = "base", value_name = "BRANCH")]
    pub base: Option<String>,

    #[arg(long = "add-reviewer", value_name = "USER")]
    pub add_reviewer: Vec<String>,

    #[arg(long = "remove-reviewer", value_name = "USER")]
    pub remove_reviewer: Vec<String>,
}

pub async fn run(args: Args, ctx: &mut Context) -> Result<(), CliError> {
    let (repo, pr) = finder::find(ctx, args.number).await?;
    let client = ctx.api().await?.clone();

    let body = read_body(&args).await?;
    let mut update = UpdatePr::default();
    if let Some(t) = args.title.clone().filter(|s| !s.is_empty()) {
        update.title = Some(t);
    }
    if let Some(b) = body {
        update.description = Some(b);
    }
    if let Some(base) = args.base.clone().filter(|s| !s.is_empty()) {
        update.destination = Some(PrEndpointInput {
            branch: BranchInput { name: base },
        });
    }

    let reviewer_change = !args.add_reviewer.is_empty() || !args.remove_reviewer.is_empty();
    if reviewer_change {
        let mut current: Vec<Actor> = pr.reviewers.clone();
        for username in &args.remove_reviewer {
            current.retain(|a| a.username.as_deref() != Some(username.as_str()));
        }
        // Re-fetch UUIDs for added reviewers.
        for username in &args.add_reviewer {
            let uuid = fetch_user_uuid(&client, username).await?;
            // Avoid dup if already present.
            if !current
                .iter()
                .any(|a| a.uuid.as_deref() == Some(uuid.as_str()))
            {
                current.push(Actor {
                    uuid: Some(uuid),
                    username: Some(username.clone()),
                    ..Default::default()
                });
            }
        }
        update.reviewers = Some(
            current
                .into_iter()
                .filter_map(|a| a.uuid.map(|uuid| ReviewerInput { uuid }))
                .collect(),
        );
    }

    if update.title.is_none()
        && update.description.is_none()
        && update.destination.is_none()
        && update.reviewers.is_none()
    {
        return Err(CliError::Flag(
            "nothing to update — pass --title, --body, --base, --add-reviewer, or --remove-reviewer"
                .into(),
        ));
    }

    client.pull_requests().update(&repo, pr.id, &update).await?;
    print_success(
        &mut ctx.io,
        &format!(
            "Updated pull request #{} ({}/{})",
            pr.id, repo.workspace, repo.slug
        ),
    )
    .map_err(|e| CliError::Other(e.into()))
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

async fn fetch_user_uuid(client: &crate::api::Client, username: &str) -> Result<String, CliError> {
    let url = client
        .base()
        .join(&format!("users/{}", urlencode(username)))
        .map_err(|e| CliError::Other(anyhow::anyhow!("invalid url: {e}")))?;
    let transport = client.transport();
    let req = transport
        .http
        .get(url)
        .build()
        .map_err(|e| CliError::Other(e.into()))?;
    let resp = transport.send(req).await?;
    let actor: Actor = resp.json()?;
    actor
        .uuid
        .ok_or_else(|| CliError::Flag(format!("user {username:?} has no UUID in API response")))
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(ch),
            _ => {
                for b in ch.to_string().bytes() {
                    out.push_str(&format!("%{b:02X}"));
                }
            }
        }
    }
    out
}
