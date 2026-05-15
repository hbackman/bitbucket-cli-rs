//! `bb repo set-default` — persist the per-clone default repo.
//!
//! Writes `bb.default-repo = workspace/slug` via `git config --local` when inside
//! a git repo. Falls back to `default_repo:` in `config.yml` when not.

use clap::Args;

use crate::bbrepo::BbRepo;
use crate::cli::messages::{print_notice, print_success};
use crate::context::Context;
use crate::error::CliError;
use crate::git;

const KEY: &str = "bb.default-repo";
const CFG_KEY: &str = "default_repo";

#[derive(Args, Debug)]
pub struct SetDefaultArgs {
    /// Repository in `WORKSPACE/REPO` form.
    pub repo: Option<String>,

    /// Print the current default and exit.
    #[arg(long)]
    pub view: bool,

    /// Remove the stored default.
    #[arg(long)]
    pub unset: bool,
}

pub async fn run(args: SetDefaultArgs, ctx: &mut Context) -> Result<(), CliError> {
    if args.view {
        return view_current(ctx).await;
    }
    if args.unset {
        return unset_default(ctx).await;
    }

    let repo = match args.repo {
        Some(s) => BbRepo::from_full_name(&s).map_err(|e| CliError::Flag(e.to_string()))?,
        None => prompt_repo(ctx).await?,
    };

    if in_git_repo().await {
        git::config_set_local(KEY, &repo.to_string())
            .await
            .map_err(CliError::Other)?;
        print_success(
            &mut ctx.io,
            &format!("Set default repo for this directory to {repo}"),
        )
        .map_err(io_err)?;
    } else {
        let cfg = ctx.config_loaded().await?;
        let mut owned = cfg.clone();
        owned
            .set(CFG_KEY, &repo.to_string())
            .await
            .map_err(CliError::Other)?;
        print_success(
            &mut ctx.io,
            &format!("Set default repo (config.yml) to {repo}"),
        )
        .map_err(io_err)?;
    }

    Ok(())
}

async fn view_current(ctx: &mut Context) -> Result<(), CliError> {
    if let Ok(val) = git::config_get(KEY).await {
        if !val.is_empty() {
            writeln!(ctx.io.out(), "{val}").map_err(io_err)?;
            return Ok(());
        }
    }
    let cfg = ctx.config_loaded().await?;
    if let Some(v) = cfg.get(CFG_KEY) {
        if !v.is_empty() {
            writeln!(ctx.io.out(), "{v}").map_err(io_err)?;
            return Ok(());
        }
    }
    print_notice(&mut ctx.io, "no default repo configured").map_err(io_err)?;
    Ok(())
}

async fn unset_default(ctx: &mut Context) -> Result<(), CliError> {
    if in_git_repo().await {
        git::config_unset_local(KEY).await.map_err(CliError::Other)?;
        print_success(&mut ctx.io, "Removed local default repo").map_err(io_err)?;
        return Ok(());
    }
    let cfg = ctx.config_loaded().await?;
    let mut owned = cfg.clone();
    owned.set(CFG_KEY, "").await.map_err(CliError::Other)?;
    print_success(&mut ctx.io, "Removed default repo from config.yml").map_err(io_err)?;
    Ok(())
}

async fn prompt_repo(ctx: &mut Context) -> Result<BbRepo, CliError> {
    if !ctx.io.is_stdin_tty() || !ctx.io.is_stdout_tty() {
        return Err(CliError::Flag(
            "missing WORKSPACE/REPO. Pass it as a positional argument.".into(),
        ));
    }
    let remotes = git::list_remotes().await.unwrap_or_default();
    let mut candidates: Vec<BbRepo> = Vec::new();
    for r in &remotes {
        if let Ok(url) = git::remote_url(r).await {
            if let Ok(rp) = BbRepo::parse_remote(&url) {
                candidates.push(rp);
            }
        }
    }
    if candidates.is_empty() {
        let prompter = ctx.prompter();
        let s = prompter.input("Repository (workspace/slug)", None)?;
        return BbRepo::from_full_name(&s).map_err(|e| CliError::Flag(e.to_string()));
    }
    let options: Vec<String> = candidates.iter().map(|c| c.to_string()).collect();
    let prompter = ctx.prompter();
    let idx = prompter.select("Which repository should be the default?", &options, 0)?;
    Ok(candidates[idx].clone())
}

async fn in_git_repo() -> bool {
    git::repo_root().await.is_ok()
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}
