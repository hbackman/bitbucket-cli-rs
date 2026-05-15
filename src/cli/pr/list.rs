//! `bb pr list` — list pull requests in the target repo.

use clap::Args as ClapArgs;
use futures::future::try_join_all;
use serde_json::Value;

use crate::api::pull_request::ListOpts;
use crate::api::types::{PrState, PullRequest};
use crate::cli::jq;
use crate::cli::json_flags::{JsonFlags, JsonMode};
use crate::context::Context;
use crate::error::CliError;
use crate::iostreams::{Column, TablePrinter};
use crate::text::{rel_time, truncate};

use super::display::{
    destination_branch, pr_html_url, project_pr, source_branch, state_icon, JSON_FIELDS,
};

const DEFAULT_LIMIT: u32 = 30;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// State filter. Repeatable. `all` selects every state. Default: `open`.
    #[arg(
        short = 's',
        long = "state",
        value_name = "STATE",
        value_parser = parse_state_filter,
    )]
    pub state: Vec<StateFilter>,

    /// Filter by author username (or `@me`).
    #[arg(short = 'a', long, value_name = "USER")]
    pub author: Option<String>,

    /// Filter by reviewer username (or `@me`). Alias: `--assignee`.
    #[arg(
        short = 'A',
        long = "assignee",
        visible_alias = "reviewer",
        value_name = "USER"
    )]
    pub reviewer: Option<String>,

    /// Filter by destination branch.
    #[arg(short = 'B', long = "base", value_name = "BRANCH")]
    pub base: Option<String>,

    /// Filter by source branch.
    #[arg(short = 'H', long = "head", value_name = "BRANCH")]
    pub head: Option<String>,

    /// Maximum number of pull requests to fetch.
    #[arg(short = 'L', long, default_value_t = DEFAULT_LIMIT)]
    pub limit: u32,

    /// Bitbucket BBQL query string passed through `?q=`.
    #[arg(long, value_name = "BBQL")]
    pub query: Option<String>,

    /// Open the PR list in the browser instead of printing.
    #[arg(short = 'w', long)]
    pub web: bool,

    #[command(flatten)]
    pub json: JsonFlags,
}

#[derive(Debug, Clone, Copy)]
pub enum StateFilter {
    Open,
    Merged,
    Declined,
    Superseded,
    All,
}

fn parse_state_filter(s: &str) -> Result<StateFilter, String> {
    match s.to_ascii_lowercase().as_str() {
        "open" => Ok(StateFilter::Open),
        "merged" => Ok(StateFilter::Merged),
        "declined" | "closed" => Ok(StateFilter::Declined),
        "superseded" => Ok(StateFilter::Superseded),
        "all" => Ok(StateFilter::All),
        other => Err(format!(
            "invalid state {other:?}. Expected one of: open, merged, declined, superseded, all"
        )),
    }
}

pub async fn run(args: Args, ctx: &mut Context) -> Result<(), CliError> {
    let mode = args.json.validate(JSON_FIELDS)?;

    if args.web {
        let repo = ctx.base_repo().await?.clone();
        let url = format!(
            "https://{}/{}/{}/pull-requests",
            repo.host, repo.workspace, repo.slug
        );
        return ctx.browser().open(&url).map_err(CliError::Other);
    }

    let repo = ctx.base_repo().await?.clone();
    let client = ctx.api().await?.clone();

    let states = effective_states(&args.state);
    let me_username = resolve_me_username(ctx, &args).await?;

    let mut tasks = Vec::with_capacity(states.len());
    for state in &states {
        let q = build_q(&args, me_username.as_deref());
        let opts = ListOpts {
            state: *state,
            query: q,
            page_len: Some(args.limit.min(50)),
            ..Default::default()
        };
        let pages = client.pull_requests().list(&repo, opts);
        let limit = args.limit as usize;
        tasks.push(async move { pages.collect(limit).await });
    }

    let results = try_join_all(tasks).await?;
    let mut prs: Vec<PullRequest> = results.into_iter().flatten().collect();
    prs.sort_by(|a, b| {
        b.updated_on
            .as_deref()
            .unwrap_or("")
            .cmp(a.updated_on.as_deref().unwrap_or(""))
    });
    prs.truncate(args.limit as usize);

    if !matches!(mode, JsonMode::Off) {
        return emit_json(ctx, &prs, &mode);
    }
    render_table(ctx, &prs)
}

fn effective_states(filters: &[StateFilter]) -> Vec<Option<PrState>> {
    if filters.is_empty() {
        return vec![Some(PrState::Open)];
    }
    let mut out = Vec::new();
    for f in filters {
        match f {
            StateFilter::Open => out.push(Some(PrState::Open)),
            StateFilter::Merged => out.push(Some(PrState::Merged)),
            StateFilter::Declined => out.push(Some(PrState::Declined)),
            StateFilter::Superseded => out.push(Some(PrState::Superseded)),
            StateFilter::All => {
                out.clear();
                out.push(Some(PrState::Open));
                out.push(Some(PrState::Merged));
                out.push(Some(PrState::Declined));
                out.push(Some(PrState::Superseded));
                break;
            }
        }
    }
    if out.is_empty() {
        out.push(Some(PrState::Open));
    }
    out
}

fn build_q(args: &Args, me: Option<&str>) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(custom) = args.query.as_deref().filter(|s| !s.is_empty()) {
        parts.push(format!("({custom})"));
    }
    if let Some(user) = at_me(args.author.as_deref(), me) {
        parts.push(format!("author.username=\"{user}\""));
    }
    if let Some(user) = at_me(args.reviewer.as_deref(), me) {
        parts.push(format!("reviewers.username=\"{user}\""));
    }
    if let Some(b) = args.base.as_deref().filter(|s| !s.is_empty()) {
        parts.push(format!("destination.branch.name=\"{b}\""));
    }
    if let Some(b) = args.head.as_deref().filter(|s| !s.is_empty()) {
        parts.push(format!("source.branch.name=\"{b}\""));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" AND "))
    }
}

fn at_me<'a>(raw: Option<&'a str>, me: Option<&'a str>) -> Option<&'a str> {
    let raw = raw.filter(|s| !s.is_empty())?;
    if raw == "@me" {
        me
    } else {
        Some(raw)
    }
}

async fn resolve_me_username(ctx: &Context, args: &Args) -> Result<Option<String>, CliError> {
    let needs_me = args.author.as_deref() == Some("@me") || args.reviewer.as_deref() == Some("@me");
    if !needs_me {
        return Ok(None);
    }
    let me = ctx.api().await?.user().current().await?;
    Ok(Some(me.username))
}

fn emit_json(ctx: &mut Context, prs: &[PullRequest], mode: &JsonMode) -> Result<(), CliError> {
    let fields = match mode {
        JsonMode::Off => return Ok(()),
        JsonMode::Fields(f) | JsonMode::FilterFields { fields: f, .. } => f.clone(),
    };
    let array = Value::Array(prs.iter().map(|p| project_pr(p, &fields)).collect());
    match mode {
        JsonMode::Off => unreachable!(),
        JsonMode::Fields(_) => {
            let s = serde_json::to_string_pretty(&array).map_err(|e| CliError::Other(e.into()))?;
            writeln!(ctx.io.out(), "{s}").map_err(io_err)
        }
        JsonMode::FilterFields { jq: expr, .. } => {
            for v in jq::run(expr, array).map_err(CliError::Other)? {
                writeln!(ctx.io.out(), "{}", jq::render(&v).map_err(CliError::Other)?)
                    .map_err(io_err)?;
            }
            Ok(())
        }
    }
}

fn render_table(ctx: &mut Context, prs: &[PullRequest]) -> Result<(), CliError> {
    let columns = vec![
        Column::new(""),
        Column::new("id"),
        Column::new("title").truncatable(),
        Column::new("branch").truncatable(),
        Column::new("updated"),
    ];
    let cs = ctx.io.cs();
    let mut t = TablePrinter::new(&mut ctx.io, columns);
    for pr in prs {
        let state = pr.state.as_deref().unwrap_or("OPEN");
        let icon = state_icon(state, &cs);
        let id = format!("#{}", pr.id);
        let title = truncate(&pr.title, 60);
        let branch = source_branch(pr).unwrap_or_default();
        let updated = pr
            .updated_on
            .as_deref()
            .and_then(parse_rfc3339)
            .map(rel_time)
            .unwrap_or_default();
        t.add_row([icon, id, title, branch, updated]);
    }
    t.render().map_err(io_err)?;
    Ok(())
}

fn parse_rfc3339(s: &str) -> Option<time::OffsetDateTime> {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::parse(s, &Rfc3339).ok()
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}

// Suppress dead_code: pr_html_url and destination_branch are re-exported.
#[allow(dead_code)]
fn _retain_helpers(_: PullRequest) {
    let _ = pr_html_url;
    let _ = destination_branch;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_states_defaults_to_open() {
        let s = effective_states(&[]);
        assert_eq!(s.len(), 1);
        assert!(matches!(s[0], Some(PrState::Open)));
    }

    #[test]
    fn effective_states_all_expands_to_four() {
        let s = effective_states(&[StateFilter::All]);
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn build_q_joins_filters() {
        let a = Args {
            state: vec![],
            author: Some("alice".into()),
            reviewer: Some("bob".into()),
            base: Some("main".into()),
            head: Some("feature".into()),
            limit: 30,
            query: None,
            web: false,
            json: JsonFlags::default(),
        };
        let q = build_q(&a, None).unwrap();
        assert!(q.contains("author.username=\"alice\""));
        assert!(q.contains("reviewers.username=\"bob\""));
        assert!(q.contains("destination.branch.name=\"main\""));
        assert!(q.contains("source.branch.name=\"feature\""));
        assert!(q.contains(" AND "));
    }

    #[test]
    fn at_me_substitutes_current_user() {
        assert_eq!(at_me(Some("@me"), Some("alice")), Some("alice"));
        assert_eq!(at_me(Some("@me"), None), None);
        assert_eq!(at_me(Some("bob"), Some("alice")), Some("bob"));
        assert_eq!(at_me(None, Some("alice")), None);
    }
}
