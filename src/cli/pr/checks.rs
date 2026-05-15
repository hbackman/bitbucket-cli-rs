//! `bbk pr checks [N]` — list build statuses, optionally watching until terminal.

use std::time::Duration;

use clap::Args as ClapArgs;

use crate::api::types::BuildStatus;
use crate::bbrepo::BbRepo;
use crate::context::Context;
use crate::error::CliError;
use crate::iostreams::{Column, TablePrinter};

use super::display::pr_html_url;
use super::finder;

const MAX_WATCH: Duration = Duration::from_secs(30 * 60);

#[derive(ClapArgs, Debug)]
pub struct Args {
    pub number: Option<u32>,

    #[arg(short = 'w', long)]
    pub web: bool,

    /// Re-poll statuses on an interval until every check is terminal.
    #[arg(long)]
    pub watch: bool,

    /// Poll interval (e.g. `5s`, `30s`). Pairs with `--watch`.
    #[arg(long, value_name = "DURATION", default_value = "5s")]
    pub interval: String,
}

pub async fn run(args: Args, ctx: &mut Context) -> Result<(), CliError> {
    let (repo, pr) = finder::find(ctx, args.number).await?;

    if args.web {
        let url = pr_html_url(&pr).unwrap_or_else(|| {
            format!(
                "https://{}/{}/{}/pull-requests/{}/builds",
                repo.host, repo.workspace, repo.slug, pr.id
            )
        });
        return ctx.browser().open(&url).map_err(CliError::Other);
    }

    if !args.watch {
        let statuses = fetch(ctx, &repo, pr.id).await?;
        render(ctx, &statuses)?;
        return summarize_exit(&statuses);
    }

    let interval: Duration = humantime::parse_duration(&args.interval)
        .map_err(|e| CliError::Flag(format!("invalid --interval {:?}: {e}", args.interval)))?;
    let interval = interval.max(Duration::from_secs(1));
    let deadline = std::time::Instant::now() + MAX_WATCH;

    loop {
        let statuses = fetch(ctx, &repo, pr.id).await?;
        render(ctx, &statuses)?;
        if statuses.iter().all(|s| is_terminal(&s.state)) {
            return summarize_exit(&statuses);
        }
        if std::time::Instant::now() >= deadline {
            return summarize_exit(&statuses);
        }
        tokio::time::sleep(interval).await;
        writeln!(ctx.io.out()).map_err(io_err)?;
    }
}

async fn fetch(ctx: &Context, repo: &BbRepo, pr_id: u32) -> Result<Vec<BuildStatus>, CliError> {
    let client = ctx.api().await?.clone();
    Ok(client
        .pull_requests()
        .statuses(repo, pr_id)
        .collect(0)
        .await?)
}

fn render(ctx: &mut Context, statuses: &[BuildStatus]) -> Result<(), CliError> {
    if statuses.is_empty() {
        writeln!(ctx.io.out(), "No build statuses reported.").map_err(io_err)?;
        return Ok(());
    }
    let cs = ctx.io.cs();
    let columns = vec![
        Column::new(""),
        Column::new("name").truncatable(),
        Column::new("state"),
        Column::new("url").truncatable(),
    ];
    let mut t = TablePrinter::new(&mut ctx.io, columns);
    for s in statuses {
        let icon = match s.state.to_ascii_uppercase().as_str() {
            "SUCCESSFUL" => cs.success_icon(),
            "FAILED" => cs.failure_icon(),
            _ => cs.neutral_icon(),
        };
        t.add_row([
            icon,
            s.name.clone().unwrap_or_else(|| s.key.clone()),
            s.state.clone(),
            s.url.clone().unwrap_or_default(),
        ]);
    }
    t.render().map_err(io_err)
}

fn summarize_exit(statuses: &[BuildStatus]) -> Result<(), CliError> {
    let any_failed = statuses
        .iter()
        .any(|s| s.state.eq_ignore_ascii_case("FAILED"));
    let any_pending = statuses.iter().any(|s| !is_terminal(&s.state));
    if any_failed || any_pending {
        Err(CliError::Silent)
    } else {
        Ok(())
    }
}

fn is_terminal(state: &str) -> bool {
    matches!(
        state.to_ascii_uppercase().as_str(),
        "SUCCESSFUL" | "FAILED" | "STOPPED" | "CANCELLED"
    )
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_state_detection() {
        assert!(is_terminal("SUCCESSFUL"));
        assert!(is_terminal("FAILED"));
        assert!(is_terminal("STOPPED"));
        assert!(!is_terminal("INPROGRESS"));
        assert!(!is_terminal("PENDING"));
    }
}
