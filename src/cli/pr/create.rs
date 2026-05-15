//! `bbk pr create` — create a new pull request.
//!
//! Steps:
//! 1. Resolve head (source) + base (destination) branches.
//! 2. Make sure the head branch exists on a Bitbucket remote — push if not (or
//!    prompt the user when interactive).
//! 3. Assemble title/body/reviewers (interactive prompts + `--fill` flags).
//! 4. POST `/pullrequests`. On failure, write a recovery JSON file with the
//!    payload so the user can replay with `--recover PATH`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Args as ClapArgs;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use crate::api::types::{
    Actor, BranchInput, CreatePr, PrEndpointInput, PullRequest, ReviewerInput,
};
use crate::bbrepo::BbRepo;
use crate::cli::messages::{print_notice, print_success, print_warning};
use crate::context::Context;
use crate::error::CliError;
use crate::git;

use super::display::pr_html_url;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(short = 't', long, value_name = "TITLE")]
    pub title: Option<String>,

    #[arg(short = 'b', long, value_name = "BODY")]
    pub body: Option<String>,

    /// Read body from a file (`-` for stdin).
    #[arg(short = 'F', long = "body-file", value_name = "PATH")]
    pub body_file: Option<String>,

    /// Destination branch. Defaults to the repo's main branch.
    #[arg(short = 'B', long = "base", value_name = "BRANCH")]
    pub base: Option<String>,

    /// Source branch. Defaults to the current git branch.
    #[arg(short = 'H', long = "head", value_name = "BRANCH")]
    pub head: Option<String>,

    /// Create as a draft.
    #[arg(short = 'd', long)]
    pub draft: bool,

    /// Reviewer username (repeatable).
    #[arg(short = 'r', long = "reviewer", value_name = "USER")]
    pub reviewers: Vec<String>,

    /// Don't delete the source branch when the PR is merged.
    #[arg(long, conflicts_with = "close_source_branch")]
    pub no_close_source_branch: bool,

    /// Delete the source branch when the PR is merged.
    #[arg(long)]
    pub close_source_branch: bool,

    /// Populate title and body from the commits between base..head.
    #[arg(short = 'f', long)]
    pub fill: bool,

    /// Populate title and body from the first commit only.
    #[arg(long = "fill-first", conflicts_with = "fill")]
    pub fill_first: bool,

    /// Open the create form in the browser instead.
    #[arg(short = 'w', long)]
    pub web: bool,

    /// Print the assembled payload without POSTing it.
    #[arg(long)]
    pub dry_run: bool,

    /// Resume a previous failed run by re-sending the recovery JSON at PATH.
    #[arg(long, value_name = "PATH")]
    pub recover: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RecoverPayload {
    repo: String,
    body: CreatePr,
}

pub async fn run(args: Args, ctx: &mut Context) -> Result<(), CliError> {
    if let Some(path) = args.recover.clone() {
        return recover(ctx, &path).await;
    }

    let repo = ctx.base_repo().await?.clone();

    if args.web {
        let url = format!(
            "https://{}/{}/{}/pull-requests/new",
            repo.host, repo.workspace, repo.slug
        );
        return ctx.browser().open(&url).map_err(CliError::Other);
    }

    let head = match args.head.clone() {
        Some(b) if !b.is_empty() => b,
        _ => git::current_branch()
            .await
            .map_err(|e| CliError::Flag(format!("could not determine head branch: {e}")))?,
    };

    let client = ctx.api().await?.clone();
    let base = match args.base.clone() {
        Some(b) if !b.is_empty() => b,
        _ => default_branch(&client, &repo).await?,
    };

    if head == base {
        return Err(CliError::Flag(format!(
            "head and base branch are the same: {head}"
        )));
    }

    ensure_head_pushed(ctx, &repo, &head).await?;

    let (title, body) = assemble_title_body(ctx, &args, &head, &base).await?;

    let mut payload = CreatePr {
        title,
        description: if body.is_empty() { None } else { Some(body) },
        source: PrEndpointInput {
            branch: BranchInput { name: head.clone() },
        },
        destination: Some(PrEndpointInput {
            branch: BranchInput { name: base.clone() },
        }),
        close_source_branch: close_source_branch(&args),
        reviewers: Vec::new(),
        draft: if args.draft { Some(true) } else { None },
    };

    // Reviewers: explicit flags + (interactive only) effective default reviewers.
    let explicit_reviewers = resolve_reviewers(ctx, &args.reviewers).await?;
    if !explicit_reviewers.is_empty() {
        payload.reviewers = explicit_reviewers;
    } else if interactive(ctx) {
        let suggestions = effective_default_reviewers(ctx, &repo)
            .await
            .unwrap_or_default();
        if !suggestions.is_empty() {
            let names: Vec<String> = suggestions
                .iter()
                .map(|a| {
                    a.username
                        .clone()
                        .unwrap_or_else(|| a.uuid.clone().unwrap_or_default())
                })
                .collect();
            let prompter = ctx.prompter();
            let picks = prompter.multi_select("Reviewers", &names, &[])?;
            payload.reviewers = picks
                .into_iter()
                .filter_map(|i| {
                    suggestions
                        .get(i)
                        .and_then(|a| a.uuid.clone())
                        .map(|uuid| ReviewerInput { uuid })
                })
                .collect();
        }
    }

    if args.dry_run {
        let rendered =
            serde_json::to_string_pretty(&payload).map_err(|e| CliError::Other(e.into()))?;
        writeln!(ctx.io.out(), "{rendered}").map_err(io_err)?;
        return Ok(());
    }

    match client.pull_requests().create(&repo, &payload).await {
        Ok(pr) => {
            let url = pr_html_url(&pr).unwrap_or_else(|| pr_fallback_url(&repo, &pr));
            print_success(
                &mut ctx.io,
                &format!(
                    "Created pull request {}/{}#{}",
                    repo.workspace, repo.slug, pr.id
                ),
            )
            .map_err(io_err)?;
            writeln!(ctx.io.out(), "{url}").map_err(io_err)?;
            Ok(())
        }
        Err(e) => {
            let path = write_recover_file(&repo, &payload)?;
            print_warning(
                &mut ctx.io,
                &format!(
                    "Failed to create pull request. Re-run with `bbk pr create --recover {}` to retry.",
                    path.display()
                ),
            )
            .map_err(io_err)?;
            Err(e.into())
        }
    }
}

async fn recover(ctx: &mut Context, path: &Path) -> Result<(), CliError> {
    let bytes = fs::read(path).map_err(|e| {
        CliError::Flag(format!(
            "could not read recovery file {}: {e}",
            path.display()
        ))
    })?;
    let payload: RecoverPayload = serde_json::from_slice(&bytes)
        .map_err(|e| CliError::Flag(format!("recovery file is not valid JSON: {e}")))?;
    let repo = BbRepo::from_full_name(&payload.repo).map_err(|e| CliError::Flag(e.to_string()))?;
    let client = ctx.api().await?.clone();
    let pr = client.pull_requests().create(&repo, &payload.body).await?;
    let url = pr_html_url(&pr).unwrap_or_else(|| pr_fallback_url(&repo, &pr));
    print_success(
        &mut ctx.io,
        &format!(
            "Created pull request {}/{}#{}",
            repo.workspace, repo.slug, pr.id
        ),
    )
    .map_err(io_err)?;
    writeln!(ctx.io.out(), "{url}").map_err(io_err)?;
    Ok(())
}

fn close_source_branch(args: &Args) -> Option<bool> {
    if args.no_close_source_branch {
        Some(false)
    } else if args.close_source_branch {
        Some(true)
    } else {
        None
    }
}

async fn default_branch(client: &crate::api::Client, repo: &BbRepo) -> Result<String, CliError> {
    let r = client.repositories().get(repo).await?;
    Ok(r.mainbranch
        .map(|b| b.name)
        .unwrap_or_else(|| "main".into()))
}

async fn ensure_head_pushed(ctx: &mut Context, _repo: &BbRepo, head: &str) -> Result<(), CliError> {
    // Identify the remote whose URL maps to the target repo. For MVP we treat
    // `origin` as the canonical remote (gh does similar; multi-remote handling
    // is in the open questions of spec 06).
    let remote = "origin";
    match git::remote_branch_exists(remote, head).await {
        Ok(true) => Ok(()),
        Ok(false) => {
            print_notice(&mut ctx.io, &format!("Pushing {head} to {remote}...")).map_err(io_err)?;
            git::push(remote, head).await.map_err(CliError::Other)
        }
        Err(_) => {
            // ls-remote may fail if the remote is unreachable; let the push attempt
            // surface the underlying error.
            print_warning(
                &mut ctx.io,
                &format!("could not verify {remote}/{head}; attempting push..."),
            )
            .map_err(io_err)?;
            git::push(remote, head).await.map_err(CliError::Other)
        }
    }
}

async fn assemble_title_body(
    ctx: &mut Context,
    args: &Args,
    head: &str,
    base: &str,
) -> Result<(String, String), CliError> {
    let mut title = args.title.clone().unwrap_or_default();
    let mut body = read_body_arg(args).await?.unwrap_or_default();

    if args.fill_first || args.fill {
        let from = base.to_string();
        let to = head.to_string();
        if args.fill_first {
            let subjects = git::log_subjects(&from, &to)
                .await
                .map_err(CliError::Other)?;
            if let Some(first) = subjects.first() {
                if title.is_empty() {
                    title = first.clone();
                }
            }
        } else {
            let messages = git::log_messages(&from, &to)
                .await
                .map_err(CliError::Other)?;
            if let Some(first) = messages.first() {
                if title.is_empty() {
                    title = first.lines().next().unwrap_or("").to_string();
                }
                if body.is_empty() {
                    let mut joined = String::new();
                    for msg in &messages {
                        joined.push_str(msg);
                        joined.push_str("\n\n");
                    }
                    body = joined.trim_end().to_string();
                }
            }
        }
    }

    if title.is_empty() {
        if !interactive(ctx) {
            return Err(CliError::Flag(
                "missing --title (or --fill / --fill-first to read from commits)".into(),
            ));
        }
        let prompter = ctx.prompter();
        title = prompter.input("Title", None)?;
        if title.trim().is_empty() {
            return Err(CliError::Flag("title cannot be empty".into()));
        }
    }

    Ok((title, body))
}

async fn read_body_arg(args: &Args) -> Result<Option<String>, CliError> {
    if let Some(b) = args.body.clone() {
        return Ok(Some(b));
    }
    let Some(path) = args.body_file.as_deref() else {
        return Ok(None);
    };
    if path == "-" {
        let mut buf = String::new();
        let mut stdin = tokio::io::stdin();
        stdin
            .read_to_string(&mut buf)
            .await
            .map_err(|e| CliError::Other(e.into()))?;
        return Ok(Some(buf));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| CliError::Flag(format!("could not read --body-file {path}: {e}")))?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

async fn resolve_reviewers(ctx: &Context, raw: &[String]) -> Result<Vec<ReviewerInput>, CliError> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let client = ctx.api().await?.clone();
    let me = if raw.iter().any(|s| s == "@me") {
        Some(client.user().current().await?.username)
    } else {
        None
    };
    let mut out = Vec::with_capacity(raw.len());
    for r in raw {
        let username = if r == "@me" {
            me.as_deref().unwrap_or(r)
        } else {
            r.as_str()
        };
        // Bitbucket expects reviewers by uuid. Look up the username → uuid mapping.
        // The user endpoint requires the bracketed UUID form, but `/users/{username}`
        // returns the actor. Use repositories::list? Simpler: hit `/users/{username}`.
        let uuid = match fetch_user_uuid(&client, username).await {
            Ok(uuid) => uuid,
            Err(e) => {
                return Err(CliError::Flag(format!(
                    "could not resolve reviewer {username:?}: {e}"
                )))
            }
        };
        out.push(ReviewerInput { uuid });
    }
    Ok(out)
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

async fn effective_default_reviewers(ctx: &Context, repo: &BbRepo) -> Option<Vec<Actor>> {
    let client = ctx.api().await.ok()?;
    let pages = client.pull_requests().effective_default_reviewers(repo);
    pages.collect(10).await.ok()
}

fn interactive(ctx: &Context) -> bool {
    ctx.io.is_stdin_tty() && ctx.io.is_stdout_tty()
}

fn pr_fallback_url(repo: &BbRepo, pr: &PullRequest) -> String {
    format!(
        "https://{}/{}/{}/pull-requests/{}",
        repo.host, repo.workspace, repo.slug, pr.id
    )
}

fn write_recover_file(repo: &BbRepo, payload: &CreatePr) -> Result<PathBuf, CliError> {
    let dirs = ProjectDirs::from("", "", "bbk")
        .ok_or_else(|| CliError::Other(anyhow::anyhow!("could not resolve a cache directory")))?;
    let dir = dirs.cache_dir().to_path_buf();
    fs::create_dir_all(&dir).map_err(|e| CliError::Other(e.into()))?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("pr-create-recover-{ts}.json"));
    let rec = RecoverPayload {
        repo: format!("{}/{}", repo.workspace, repo.slug),
        body: payload.clone(),
    };
    let serialized = serde_json::to_vec_pretty(&rec).map_err(|e| CliError::Other(e.into()))?;
    fs::write(&path, &serialized).map_err(|e| CliError::Other(e.into()))?;
    Ok(path)
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_source_branch_resolution() {
        let mut a = Args::default_for_tests();
        a.close_source_branch = true;
        assert_eq!(close_source_branch(&a), Some(true));
        a.close_source_branch = false;
        a.no_close_source_branch = true;
        assert_eq!(close_source_branch(&a), Some(false));
        a.no_close_source_branch = false;
        assert_eq!(close_source_branch(&a), None);
    }

    impl Args {
        fn default_for_tests() -> Self {
            Self {
                title: None,
                body: None,
                body_file: None,
                base: None,
                head: None,
                draft: false,
                reviewers: vec![],
                no_close_source_branch: false,
                close_source_branch: false,
                fill: false,
                fill_first: false,
                web: false,
                dry_run: false,
                recover: None,
            }
        }
    }
}
