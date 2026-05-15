//! `bbk repo view` — print repo metadata + README.

use clap::Args;

use super::display::{project_repo, repo_html_url, JSON_FIELDS};
use crate::api::types::Repository;
use crate::bbrepo::BbRepo;
use crate::cli::jq;
use crate::cli::json_flags::{JsonFlags, JsonMode};
use crate::cli::markdown;
use crate::context::Context;
use crate::error::CliError;

const README_CANDIDATES: &[&str] = &["README.md", "README.rst", "README.txt", "README"];

#[derive(Args, Debug)]
pub struct ViewArgs {
    /// `WORKSPACE/REPO`. Optional — defaults to the current repo.
    pub repo: Option<String>,

    /// Print the README rendered from this branch instead of the default branch.
    #[arg(short = 'b', long, value_name = "BRANCH")]
    pub branch: Option<String>,

    /// Open in the browser instead of printing.
    #[arg(short = 'w', long)]
    pub web: bool,

    #[command(flatten)]
    pub json: JsonFlags,
}

pub async fn run(args: ViewArgs, ctx: &mut Context) -> Result<(), CliError> {
    let repo = resolve_repo(ctx, args.repo.as_deref()).await?;
    let mode = args.json.validate(JSON_FIELDS)?;

    let client = ctx.api().await?.clone();
    let repository = client.repositories().get(&repo).await?;

    if args.web {
        let url = repo_html_url(&repository)
            .unwrap_or_else(|| format!("https://{}/{}", repo.host, repository.full_name));
        let browser = ctx.browser();
        browser.open(&url).map_err(CliError::Other)?;
        return Ok(());
    }

    if let Some(out) = emit_json(ctx, &repository, &mode)? {
        return out;
    }

    render_human(ctx, &repository).await?;

    let branch = args
        .branch
        .clone()
        .or_else(|| repository.mainbranch.as_ref().map(|b| b.name.clone()))
        .unwrap_or_else(|| "main".to_string());
    let readme = fetch_readme(&client, &repo, &branch).await;
    if let Some(body) = readme {
        let cs = ctx.io.cs();
        let rendered = markdown::render(body.trim_end(), &cs);
        writeln!(ctx.io.out()).map_err(io_err)?;
        writeln!(ctx.io.out(), "{rendered}").map_err(io_err)?;
    }

    Ok(())
}

async fn resolve_repo(ctx: &Context, explicit: Option<&str>) -> Result<BbRepo, CliError> {
    if let Some(s) = explicit {
        return BbRepo::from_full_name(s).map_err(|e| CliError::Flag(e.to_string()));
    }
    Ok(ctx.base_repo().await?.clone())
}

fn emit_json(
    ctx: &mut Context,
    repository: &Repository,
    mode: &JsonMode,
) -> Result<Option<Result<(), CliError>>, CliError> {
    match mode {
        JsonMode::Off => Ok(None),
        JsonMode::Fields(fields) => {
            let value = project_repo(repository, fields);
            let rendered =
                serde_json::to_string_pretty(&value).map_err(|e| CliError::Other(e.into()))?;
            writeln!(ctx.io.out(), "{rendered}").map_err(io_err)?;
            Ok(Some(Ok(())))
        }
        JsonMode::FilterFields { fields, jq: expr } => {
            let value = project_repo(repository, fields);
            for out in jq::run(expr, value).map_err(CliError::Other)? {
                writeln!(
                    ctx.io.out(),
                    "{}",
                    jq::render(&out).map_err(CliError::Other)?
                )
                .map_err(io_err)?;
            }
            Ok(Some(Ok(())))
        }
    }
}

async fn render_human(ctx: &mut Context, r: &Repository) -> Result<(), CliError> {
    let cs = ctx.io.cs();
    let visibility = if r.is_private { "Private" } else { "Public" };
    let branch = r
        .mainbranch
        .as_ref()
        .map(|b| b.name.as_str())
        .unwrap_or("—");

    let url = repo_html_url(r).unwrap_or_default();
    let title = cs.bold(&r.full_name);
    let title = if url.is_empty() {
        title
    } else {
        cs.hyperlink(&title, &url)
    };
    writeln!(ctx.io.out(), "{title}").map_err(io_err)?;
    let mut line = format!("{visibility} • {branch}");
    if let Some(lang) = &r.language {
        if !lang.is_empty() {
            line.push_str(&format!(" • {lang}"));
        }
    }
    writeln!(ctx.io.out(), "{line}").map_err(io_err)?;
    if let Some(desc) = r.description.as_deref() {
        if !desc.is_empty() {
            writeln!(ctx.io.out()).map_err(io_err)?;
            writeln!(ctx.io.out(), "{desc}").map_err(io_err)?;
        }
    }
    Ok(())
}

async fn fetch_readme(client: &crate::api::Client, repo: &BbRepo, branch: &str) -> Option<String> {
    let svc = client.repositories();
    for name in README_CANDIDATES {
        match svc.read_source(repo, branch, name).await {
            Ok(bytes) => {
                if let Ok(s) = std::str::from_utf8(&bytes) {
                    return Some(
                        prefix_lines(s, "  ")
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join(""),
                    );
                }
            }
            Err(_) => continue,
        }
    }
    None
}

fn prefix_lines(s: &str, prefix: &str) -> Vec<String> {
    s.split_inclusive('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect()
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}
