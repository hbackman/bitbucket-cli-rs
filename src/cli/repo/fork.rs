//! `bb repo fork` — fork a repository on Bitbucket Cloud.

use std::time::Duration;

use clap::Args;

use crate::api::repository::{ForkInput, WorkspaceRef};
use crate::api::types::Repository;
use crate::bbrepo::BbRepo;
use crate::cli::messages::{print_notice, print_success, print_warning};
use crate::context::Context;
use crate::error::CliError;
use crate::{config, git};

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const POLL_DEADLINE: Duration = Duration::from_secs(30);

#[derive(Args, Debug)]
pub struct ForkArgs {
    /// Source repo, `WORKSPACE/REPO`. Defaults to the current repo.
    pub repo: Option<String>,

    /// Target workspace. Defaults to the current user's workspace.
    #[arg(long, value_name = "WORKSPACE")]
    pub org: Option<String>,

    /// Name for the fork. Defaults to the source repo name.
    #[arg(long, value_name = "NAME")]
    pub fork_name: Option<String>,

    /// Clone the fork into the current directory after creating it.
    #[arg(long)]
    pub clone: bool,

    /// When inside a clone of the parent repo, add the fork as a remote (default true).
    #[arg(long)]
    pub remote: bool,

    /// Name of the remote to add. Default `origin`.
    #[arg(long, value_name = "NAME", default_value = "origin")]
    pub remote_name: String,

    /// Fork only the default branch (Bitbucket option).
    #[arg(long)]
    pub default_branch_only: bool,
}

pub async fn run(args: ForkArgs, ctx: &mut Context) -> Result<(), CliError> {
    let source = resolve_source(ctx, args.repo.as_deref()).await?;
    let client = ctx.api().await?.clone();

    let owner_workspace = match &args.org {
        Some(s) if !s.is_empty() => s.clone(),
        _ => {
            client
                .user()
                .current()
                .await
                .map_err(CliError::from)?
                .username
        }
    };

    let mut body = ForkInput::default();
    if let Some(name) = args.fork_name.clone() {
        body.name = Some(name);
    }
    body.workspace = Some(WorkspaceRef {
        slug: owner_workspace.clone(),
    });
    if args.default_branch_only {
        body.fork_policy = Some("no_public_forks".into());
    }

    let fork: Repository = client.repositories().fork(&source, &body).await?;
    print_success(
        &mut ctx.io,
        &format!("Created fork {} from {}", fork.full_name, source),
    )
    .map_err(io_err)?;

    let fork_repo = parse_fork_repo(&fork, &owner_workspace, &source);
    wait_for_fork_ready(&client, &fork_repo).await;

    // If we're in a clone of the source repo, rewire the remotes.
    if args.remote {
        if let Err(e) = rewire_remotes(ctx, &source, &fork_repo, &args).await {
            print_warning(&mut ctx.io, &format!("could not update remotes: {e}"))
                .map_err(io_err)?;
        }
    }

    if args.clone {
        let cfg = ctx.config_loaded().await?;
        let protocol = cfg.get_or_default("git_protocol");
        let url = super::clone::clone_url_for(&fork_repo, &protocol);
        git::clone(&url, None, &[]).await.map_err(CliError::Other)?;
    }

    Ok(())
}

async fn resolve_source(ctx: &Context, explicit: Option<&str>) -> Result<BbRepo, CliError> {
    if let Some(s) = explicit {
        return BbRepo::from_full_name(s).map_err(|e| CliError::Flag(e.to_string()));
    }
    Ok(ctx.base_repo().await?.clone())
}

fn parse_fork_repo(fork: &Repository, fallback_ws: &str, source: &BbRepo) -> BbRepo {
    if let Some((ws, slug)) = fork.full_name.split_once('/') {
        return BbRepo::with_host(ws, slug, source.host.clone());
    }
    BbRepo::with_host(fallback_ws, fork.name.clone(), source.host.clone())
}

async fn wait_for_fork_ready(client: &crate::api::Client, fork: &BbRepo) {
    let started = std::time::Instant::now();
    while started.elapsed() < POLL_DEADLINE {
        match client.repositories().get(fork).await {
            Ok(r) if r.created_on.is_some() => return,
            _ => {}
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn rewire_remotes(
    ctx: &mut Context,
    source: &BbRepo,
    fork: &BbRepo,
    args: &ForkArgs,
) -> Result<(), CliError> {
    // Only act when we're inside a checkout whose origin points at the source.
    let origin_url = match git::remote_url("origin").await {
        Ok(url) => url,
        Err(_) => return Ok(()),
    };
    let origin_repo = BbRepo::parse_remote(&origin_url).ok();
    if origin_repo.as_ref() != Some(source) {
        return Ok(());
    }

    let cfg = ctx.config_loaded().await?;
    let protocol = cfg.get_or_default("git_protocol");
    let fork_url = super::clone::clone_url_for(fork, &protocol);

    git::remote_rename("origin", "upstream")
        .await
        .map_err(CliError::Other)?;
    git::remote_add(&args.remote_name, &fork_url)
        .await
        .map_err(CliError::Other)?;
    print_notice(
        &mut ctx.io,
        &format!(
            "Renamed origin → upstream, added {} → {}",
            args.remote_name, fork_url
        ),
    )
    .map_err(io_err)?;
    let _ = config::DEFAULT_HOST;
    Ok(())
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}
