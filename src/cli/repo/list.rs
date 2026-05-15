//! `bbk repo list` — list repos in a workspace or accessible to the current user.

use clap::Args;
use serde_json::Value;

use super::display::{project_repo, JSON_FIELDS};
use crate::api::repository::ListOpts;
use crate::api::types::Repository;
use crate::cli::jq;
use crate::cli::json_flags::{JsonFlags, JsonMode};
use crate::context::Context;
use crate::error::CliError;
use crate::iostreams::spinner::Spinner;
use crate::iostreams::{Column, TablePrinter};
use crate::text::{rel_time, truncate};

const DEFAULT_LIMIT: usize = 30;

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Workspace slug. If omitted, lists repos accessible to the current user.
    pub workspace: Option<String>,

    /// Maximum number of repos to print.
    #[arg(short = 'L', long, default_value_t = DEFAULT_LIMIT as u32)]
    pub limit: u32,

    /// Filter by visibility (`public` or `private`).
    #[arg(long, value_name = "VIS")]
    pub visibility: Option<String>,

    /// Filter by role (`owner`, `admin`, `member`, `contributor`).
    #[arg(long, value_name = "ROLE")]
    pub role: Option<String>,

    /// Filter by primary language.
    #[arg(long, value_name = "LANG")]
    pub language: Option<String>,

    /// Sort key: `updated`, `created`, `name`.
    #[arg(long, value_name = "KEY")]
    pub sort: Option<String>,

    #[command(flatten)]
    pub json: JsonFlags,
}

pub async fn run(args: ListArgs, ctx: &mut Context) -> Result<(), CliError> {
    let mode = args.json.validate(JSON_FIELDS)?;
    let client = ctx.api().await?.clone();

    let workspace = match args.workspace.clone() {
        Some(w) if !w.is_empty() => Some(w),
        _ => None,
    };

    let stderr_tty = ctx.io.is_stderr_tty();
    let repos = if let Some(ws) = workspace {
        let spin = Spinner::start(format!("Listing repos in {ws}…"), stderr_tty);
        let opts = build_list_opts(&args);
        let pages = client.repositories().list(&ws, opts);
        let result = pages.collect(args.limit as usize).await;
        spin.stop();
        result?
    } else {
        let spin = Spinner::start("Listing repos across your workspaces…", stderr_tty);
        let result = list_accessible_to_user(&client, &args).await;
        spin.stop();
        result?
    };

    let repos = apply_local_filters(repos, &args);

    if !matches!(mode, JsonMode::Off) {
        return emit_json(ctx, &repos, &mode);
    }

    render_table(ctx, &repos)
}

fn build_list_opts(args: &ListArgs) -> ListOpts {
    let mut q_terms: Vec<String> = Vec::new();
    match args.visibility.as_deref() {
        Some("public") => q_terms.push("is_private = false".into()),
        Some("private") => q_terms.push("is_private = true".into()),
        _ => {}
    }
    if let Some(lang) = args.language.as_deref() {
        if !lang.is_empty() {
            q_terms.push(format!("language = \"{lang}\""));
        }
    }
    ListOpts {
        query: if q_terms.is_empty() {
            None
        } else {
            Some(q_terms.join(" AND "))
        },
        role: args.role.clone(),
        pagelen: None,
        fields: None,
        sort: args.sort.as_ref().map(|s| sort_key(s)),
    }
}

fn sort_key(s: &str) -> String {
    match s {
        "updated" => "-updated_on".into(),
        "created" => "-created_on".into(),
        "name" => "name".into(),
        other => other.to_string(),
    }
}

async fn list_accessible_to_user(
    client: &crate::api::Client,
    args: &ListArgs,
) -> Result<Vec<Repository>, CliError> {
    // Bitbucket CHANGE-2770 retired the unscoped `GET /2.0/repositories` endpoint.
    // The replacement: enumerate the user's workspaces, then list repos in each
    // (`GET /2.0/repositories/{workspace}?role=…`) and concatenate. We stop early
    // once we've collected `limit` repos.
    let role = args
        .role
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let limit = args.limit as usize;

    let memberships = client.workspaces().list().collect(0).await?;
    let mut out: Vec<Repository> = Vec::with_capacity(limit);
    for m in memberships {
        if out.len() >= limit {
            break;
        }
        let opts = crate::api::repository::ListOpts {
            role: role.clone(),
            sort: args.sort.as_deref().map(sort_key),
            ..Default::default()
        };
        let remaining = limit - out.len();
        let page = client
            .repositories()
            .list(&m.workspace.slug, opts)
            .collect(remaining)
            .await?;
        out.extend(page);
    }
    out.truncate(limit);
    Ok(out)
}

fn apply_local_filters(mut repos: Vec<Repository>, args: &ListArgs) -> Vec<Repository> {
    if let Some(lang) = args.language.as_deref() {
        if !lang.is_empty() {
            repos.retain(|r| r.language.as_deref() == Some(lang));
        }
    }
    match args.visibility.as_deref() {
        Some("public") => repos.retain(|r| !r.is_private),
        Some("private") => repos.retain(|r| r.is_private),
        _ => {}
    }
    repos
}

fn emit_json(ctx: &mut Context, repos: &[Repository], mode: &JsonMode) -> Result<(), CliError> {
    let projected: Vec<Value> = match mode {
        JsonMode::Off => return Ok(()),
        JsonMode::Fields(fields) | JsonMode::FilterFields { fields, .. } => {
            repos.iter().map(|r| project_repo(r, fields)).collect()
        }
    };
    let array = Value::Array(projected);

    match mode {
        JsonMode::Off => unreachable!(),
        JsonMode::Fields(_) => {
            let rendered =
                serde_json::to_string_pretty(&array).map_err(|e| CliError::Other(e.into()))?;
            writeln!(ctx.io.out(), "{rendered}").map_err(io_err)
        }
        JsonMode::FilterFields { jq: expr, .. } => {
            for out in jq::run(expr, array).map_err(CliError::Other)? {
                writeln!(
                    ctx.io.out(),
                    "{}",
                    jq::render(&out).map_err(CliError::Other)?
                )
                .map_err(io_err)?;
            }
            Ok(())
        }
    }
}

fn render_table(ctx: &mut Context, repos: &[Repository]) -> Result<(), CliError> {
    let columns = vec![
        Column::new("name"),
        Column::new("visibility"),
        Column::new("description").truncatable(),
        Column::new("updated"),
    ];
    let mut t = TablePrinter::new(&mut ctx.io, columns);
    for r in repos {
        let updated = r
            .updated_on
            .as_deref()
            .and_then(parse_rfc3339)
            .map(rel_time)
            .unwrap_or_default();
        let desc = r
            .description
            .clone()
            .map(|d| truncate(&d, 60))
            .unwrap_or_default();
        let visibility = if r.is_private { "private" } else { "public" };
        t.add_row([r.full_name.clone(), visibility.to_string(), desc, updated]);
    }
    t.render().map_err(io_err)
}

fn parse_rfc3339(s: &str) -> Option<time::OffsetDateTime> {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::parse(s, &Rfc3339).ok()
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}
