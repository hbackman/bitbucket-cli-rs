//! `bb repo clone WS/REPO [DIR] [-- git-args]`.

use clap::Args;

use crate::api::types::Repository;
use crate::bbrepo::BbRepo;
use crate::cli::messages::print_warning;
use crate::context::Context;
use crate::error::CliError;
use crate::{config, git};

#[derive(Args, Debug)]
pub struct CloneArgs {
    /// Repository in `WORKSPACE/REPO` form.
    pub repo: String,

    /// Optional directory name to clone into.
    pub directory: Option<String>,

    /// Name for the upstream remote when cloning a fork. Default `upstream`.
    #[arg(short = 'u', long = "upstream-remote-name", default_value = "upstream")]
    pub upstream_remote_name: String,

    /// Don't add the `upstream` remote even when the repo is a fork.
    #[arg(long)]
    pub no_upstream: bool,

    /// Extra args forwarded to `git clone`. Use `--` to separate.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub extra: Vec<String>,
}

pub async fn run(args: CloneArgs, ctx: &mut Context) -> Result<(), CliError> {
    let repo = BbRepo::from_full_name(&args.repo).map_err(|e| CliError::Flag(e.to_string()))?;
    let cfg = ctx.config_loaded().await?;
    let protocol = cfg.get_or_default("git_protocol");
    let url = clone_url(&repo, &protocol);

    let extra = strip_separator(&args.extra);
    git::clone(&url, args.directory.as_deref(), &extra)
        .await
        .map_err(CliError::Other)?;

    if args.no_upstream {
        return Ok(());
    }

    // Fetch repo to learn whether it's a fork.
    let client = ctx.api().await?.clone();
    let metadata = client.repositories().get(&repo).await?;
    let parent = parent_repo(&metadata);
    if let Some((ws, slug)) = parent {
        let parent_repo = BbRepo::with_host(ws.clone(), slug.clone(), repo.host.clone());
        let upstream_url = clone_url(&parent_repo, &protocol);
        let target_dir = args.directory.clone().unwrap_or_else(|| repo.slug.clone());
        if let Err(e) = git::remote_add_in(
            std::path::Path::new(&target_dir),
            &args.upstream_remote_name,
            &upstream_url,
        )
        .await
        {
            print_warning(&mut ctx.io, &format!("could not add upstream remote: {e}"))
                .map_err(io_err)?;
        }
    }

    Ok(())
}

/// Strip a leading `--` from the trailing-var-arg vec (clap doesn't drop it).
fn strip_separator(extra: &[String]) -> Vec<String> {
    let mut iter = extra.iter().peekable();
    if iter.peek().map(|s| s.as_str()) == Some("--") {
        iter.next();
    }
    iter.cloned().collect()
}

fn clone_url(repo: &BbRepo, protocol: &str) -> String {
    clone_url_for(repo, protocol)
}

/// Build a clone URL for `repo` using the given git protocol. Public so other
/// repo commands (`create`, `fork`) can share the synthesis.
pub fn clone_url_for(repo: &BbRepo, protocol: &str) -> String {
    let host = if repo.host.is_empty() {
        config::DEFAULT_HOST
    } else {
        &repo.host
    };
    match protocol {
        "ssh" => format!("git@{host}:{}/{}.git", repo.workspace, repo.slug),
        _ => format!("https://{host}/{}/{}.git", repo.workspace, repo.slug),
    }
}

fn parent_repo(r: &Repository) -> Option<(String, String)> {
    let parent = r.parent.as_ref()?;
    let full = parent.full_name.as_deref()?;
    let (ws, slug) = full.split_once('/')?;
    Some((ws.to_string(), slug.to_string()))
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}
