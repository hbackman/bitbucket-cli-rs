//! `bbk repo create` — create a new repository on Bitbucket Cloud.

use std::path::{Path, PathBuf};

use clap::Args;

use crate::api::repository::{CreateRepo, MainBranchInput};
use crate::bbrepo::BbRepo;
use crate::cli::messages::print_success;
use crate::context::Context;
use crate::error::CliError;
use crate::{config, git};

#[derive(Args, Debug)]
pub struct CreateArgs {
    /// Repository name, or `WORKSPACE/NAME` to set both at once.
    pub name: Option<String>,

    #[arg(short = 'd', long, value_name = "TEXT")]
    pub description: Option<String>,

    #[arg(long, value_name = "URL")]
    pub homepage: Option<String>,

    /// Create as public. Mutually exclusive with `--private`.
    #[arg(long, conflicts_with = "private")]
    pub public: bool,

    /// Create as private. Default when neither `--public` nor `--private` is set.
    #[arg(long)]
    pub private: bool,

    /// Local directory to push to the new repo.
    #[arg(long, value_name = "DIR")]
    pub source: Option<PathBuf>,

    /// After creating, push the contents of `--source`.
    #[arg(long)]
    pub push: bool,

    /// Remote name to use when pushing (default `origin`).
    #[arg(long, value_name = "NAME", default_value = "origin")]
    pub remote: String,

    /// Target workspace. Defaults to the current user's workspace.
    #[arg(long, value_name = "WORKSPACE")]
    pub team: Option<String>,

    /// Clone the new repo into the current directory after creating it.
    #[arg(long)]
    pub clone: bool,

    /// Initialize with a README file. (Bitbucket-side; ignored for empty repos.)
    #[arg(long)]
    pub add_readme: bool,

    /// `.gitignore` template — Bitbucket-side, accepted but only sent through.
    #[arg(long, value_name = "TEMPLATE")]
    pub gitignore: Option<String>,

    /// License template — accepted but only sent through.
    #[arg(long, value_name = "TEMPLATE")]
    pub license: Option<String>,

    /// Default branch name.
    #[arg(long, value_name = "NAME", default_value = "main")]
    pub default_branch: String,
}

pub async fn run(args: CreateArgs, ctx: &mut Context) -> Result<(), CliError> {
    let (workspace, slug) = resolve_target(ctx, &args).await?;
    let repo = BbRepo::with_host(workspace.clone(), slug.clone(), config::DEFAULT_HOST);

    let mut body = CreateRepo::new();
    body.description = args.description.clone();
    body.is_private = !args.public; // private unless --public
    body.mainbranch = Some(MainBranchInput {
        name: args.default_branch.clone(),
        kind: "branch",
    });

    let client = ctx.api().await?.clone();
    let created = client.repositories().create(&repo, &body).await?;

    let url_hint = super::display::repo_html_url(&created)
        .unwrap_or_else(|| format!("https://{}/{}", repo.host, created.full_name));
    print_success(
        &mut ctx.io,
        &format!("Created repository {} at {}", created.full_name, url_hint),
    )
    .map_err(io_err)?;

    if let Some(source) = args.source.as_deref() {
        let pushed = setup_local(source, &repo, &args, ctx).await?;
        if pushed {
            print_success(
                &mut ctx.io,
                &format!("Pushed {} to {} on {}", source.display(), args.remote, repo),
            )
            .map_err(io_err)?;
        }
    }

    if args.clone {
        let cfg = ctx.config_loaded().await?;
        let protocol = cfg.get_or_default("git_protocol");
        let url = super::clone::clone_url_for(&repo, &protocol);
        git::clone(&url, None, &[]).await.map_err(CliError::Other)?;
    }

    Ok(())
}

async fn resolve_target(
    ctx: &mut Context,
    args: &CreateArgs,
) -> Result<(String, String), CliError> {
    if let Some(name) = args.name.clone() {
        if let Some((ws, slug)) = name.split_once('/') {
            return Ok((ws.to_string(), slug.to_string()));
        }
        let workspace = match args.team.clone() {
            Some(t) if !t.is_empty() => t,
            _ => default_workspace(ctx).await?,
        };
        return Ok((workspace, name));
    }
    // No name passed — prompt interactively.
    if !ctx.io.is_stdin_tty() || !ctx.io.is_stdout_tty() {
        return Err(CliError::Flag(
            "missing repository name. Pass `WORKSPACE/NAME` or `NAME` (with --team).".into(),
        ));
    }
    let prompter = ctx.prompter();
    let workspace = match args.team.clone() {
        Some(t) if !t.is_empty() => t,
        _ => prompter.input(
            "Workspace",
            Some(&default_workspace(ctx).await.unwrap_or_default()),
        )?,
    };
    let name = prompter.input("Repository name", None)?;
    Ok((workspace, name))
}

async fn default_workspace(ctx: &Context) -> Result<String, CliError> {
    let client = ctx.api().await?.clone();
    let user = client.user().current().await.map_err(CliError::from)?;
    Ok(user.username)
}

async fn setup_local(
    source: &Path,
    repo: &BbRepo,
    args: &CreateArgs,
    ctx: &mut Context,
) -> Result<bool, CliError> {
    if !source.exists() {
        return Err(CliError::Flag(format!(
            "--source path does not exist: {}",
            source.display()
        )));
    }
    git::init(source).await.map_err(CliError::Other)?;
    let cfg = ctx.config_loaded().await?;
    let protocol = cfg.get_or_default("git_protocol");
    let url = super::clone::clone_url_for(repo, &protocol);
    if let Err(e) = git::remote_add_in(source, &args.remote, &url).await {
        // A remote with the same name might exist already — treat as non-fatal.
        let _ = e;
    }
    if !args.push {
        return Ok(false);
    }
    git::push_set_upstream(source, &args.remote, &args.default_branch)
        .await
        .map_err(CliError::Other)?;
    Ok(true)
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}
