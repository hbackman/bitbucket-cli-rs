//! `bbk pr view [N]` — show details of a pull request.

use clap::Args as ClapArgs;

use crate::api::types::{Comment, PullRequest};
use crate::cli::jq;
use crate::cli::json_flags::{JsonFlags, JsonMode};
use crate::context::Context;
use crate::error::CliError;
use crate::text::{pluralize, rel_time};

use super::display::{
    actor_display, destination_branch, pr_html_url, project_pr, source_branch, state_colored,
    JSON_FIELDS,
};
use super::finder;
use crate::cli::markdown;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Pull request number. Optional — defaults to the PR for the current branch.
    pub number: Option<u32>,

    /// Print comments instead of the body.
    #[arg(short = 'c', long)]
    pub comments: bool,

    /// Open the PR in the browser.
    #[arg(short = 'w', long)]
    pub web: bool,

    #[command(flatten)]
    pub json: JsonFlags,
}

pub async fn run(args: Args, ctx: &mut Context) -> Result<(), CliError> {
    let mode = args.json.validate(JSON_FIELDS)?;
    let (_repo, pr) = finder::find(ctx, args.number).await?;

    if args.web {
        let url = pr_html_url(&pr).unwrap_or_else(|| fallback_url(ctx, &pr));
        return ctx.browser().open(&url).map_err(CliError::Other);
    }

    if !matches!(mode, JsonMode::Off) {
        return emit_json(ctx, &pr, &mode);
    }

    if args.comments {
        return print_comments(ctx, &pr).await;
    }

    render_human(ctx, &pr)
}

fn fallback_url(_ctx: &Context, pr: &PullRequest) -> String {
    format!(
        "https://bitbucket.org/{}/pull-requests/{}",
        repo_full_name(pr).unwrap_or_default(),
        pr.id
    )
}

fn repo_full_name(pr: &PullRequest) -> Option<String> {
    pr.destination
        .as_ref()
        .and_then(|e| e.repository.as_ref())
        .and_then(|r| r.full_name.clone())
}

fn emit_json(ctx: &mut Context, pr: &PullRequest, mode: &JsonMode) -> Result<(), CliError> {
    let fields = match mode {
        JsonMode::Off => return Ok(()),
        JsonMode::Fields(f) | JsonMode::FilterFields { fields: f, .. } => f.clone(),
    };
    let value = project_pr(pr, &fields);
    match mode {
        JsonMode::Off => unreachable!(),
        JsonMode::Fields(_) => {
            let s = serde_json::to_string_pretty(&value).map_err(|e| CliError::Other(e.into()))?;
            writeln!(ctx.io.out(), "{s}").map_err(io_err)
        }
        JsonMode::FilterFields { jq: expr, .. } => {
            for v in jq::run(expr, value).map_err(CliError::Other)? {
                writeln!(ctx.io.out(), "{}", jq::render(&v).map_err(CliError::Other)?)
                    .map_err(io_err)?;
            }
            Ok(())
        }
    }
}

fn render_human(ctx: &mut Context, pr: &PullRequest) -> Result<(), CliError> {
    let cs = ctx.io.cs();
    let title = cs.bold(&pr.title);
    let id = cs.gray(format!("#{}", pr.id));
    writeln!(ctx.io.out(), "{title} {id}").map_err(io_err)?;

    let state = pr.state.as_deref().unwrap_or("OPEN");
    let state_str = state_colored(state, &cs);
    let opened = pr
        .created_on
        .as_deref()
        .and_then(parse_rfc3339)
        .map(rel_time)
        .unwrap_or_default();
    let by = actor_display(pr.author.as_ref());
    let draft = if pr.draft {
        format!(" • {}", cs.gray("Draft"))
    } else {
        String::new()
    };
    writeln!(ctx.io.out(), "{state_str} • opened {opened} by {by}{draft}").map_err(io_err)?;

    let src = source_branch(pr).unwrap_or_else(|| "?".into());
    let dst = destination_branch(pr).unwrap_or_else(|| "?".into());
    writeln!(ctx.io.out(), "{src} → {dst}").map_err(io_err)?;

    let url = pr_html_url(pr).unwrap_or_else(|| fallback_url(ctx, pr));
    let linked_url = cs.hyperlink(&url, &url);
    writeln!(ctx.io.out(), "{}: {linked_url}", cs.gray("URL")).map_err(io_err)?;

    if !pr.reviewers.is_empty() {
        let names: Vec<String> = pr
            .reviewers
            .iter()
            .map(|a| actor_display(Some(a)))
            .collect();
        writeln!(ctx.io.out()).map_err(io_err)?;
        writeln!(ctx.io.out(), "{}", section(&cs, "Reviewers")).map_err(io_err)?;
        writeln!(ctx.io.out(), "  {}", names.join(", ")).map_err(io_err)?;
    }

    if let Some(n) = pr.comment_count {
        if n > 0 {
            writeln!(ctx.io.out()).map_err(io_err)?;
            writeln!(ctx.io.out(), "{}", section(&cs, "Comments")).map_err(io_err)?;
            writeln!(ctx.io.out(), "  {}", pluralize(n as i64, "comment")).map_err(io_err)?;
        }
    }

    if let Some(body) = pr.description.as_deref().filter(|s| !s.is_empty()) {
        writeln!(ctx.io.out()).map_err(io_err)?;
        writeln!(ctx.io.out(), "{}", section(&cs, "Description")).map_err(io_err)?;
        let rendered = markdown::render(body, &cs);
        writeln!(ctx.io.out(), "{rendered}").map_err(io_err)?;
    }
    Ok(())
}

/// Bold cyan section header used to break up `pr view` blocks.
fn section(cs: &crate::iostreams::ColorScheme, label: &str) -> String {
    cs.bold(cs.cyan(label))
}

async fn print_comments(ctx: &mut Context, pr: &PullRequest) -> Result<(), CliError> {
    let repo = ctx.base_repo().await?.clone();
    let client = ctx.api().await?.clone();
    let comments: Vec<Comment> = client
        .pull_requests()
        .comments(&repo, pr.id)
        .collect(0)
        .await?;
    if comments.is_empty() {
        writeln!(ctx.io.out(), "No comments.").map_err(io_err)?;
        return Ok(());
    }
    let cs = ctx.io.cs();
    for c in &comments {
        let author = actor_display(c.user.as_ref());
        let when = c
            .created_on
            .as_deref()
            .and_then(parse_rfc3339)
            .map(rel_time)
            .unwrap_or_default();
        let where_ = c
            .inline
            .as_ref()
            .map(|loc| format!(" — {}:{}", loc.path, loc.to.unwrap_or(0)))
            .unwrap_or_default();
        writeln!(
            ctx.io.out(),
            "{} {} {}{}",
            cs.bold(&author),
            cs.gray(when),
            cs.gray(format!("comment #{}", c.id)),
            where_,
        )
        .map_err(io_err)?;
        let body = c
            .content
            .as_ref()
            .and_then(|x| x.raw.clone())
            .unwrap_or_default();
        if !body.is_empty() {
            writeln!(ctx.io.out(), "{}", markdown::render(&body, &cs)).map_err(io_err)?;
        }
        writeln!(ctx.io.out()).map_err(io_err)?;
    }
    Ok(())
}

fn parse_rfc3339(s: &str) -> Option<time::OffsetDateTime> {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::parse(s, &Rfc3339).ok()
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}
