//! `bb version` — print the build info, with optional `--json`. Also runs the
//! 24-hour update notifier unless disabled.

use clap::Args;
use serde_json::json;

use crate::context::Context;
use crate::error::CliError;
use crate::update;

#[derive(Args, Debug, Default)]
pub struct VersionArgs {
    /// Print version metadata as JSON.
    #[arg(long)]
    pub json: bool,
}

pub async fn run(ctx: &mut Context) -> Result<(), CliError> {
    run_with(VersionArgs::default(), ctx).await
}

pub async fn run_with(args: VersionArgs, ctx: &mut Context) -> Result<(), CliError> {
    if args.json {
        let payload = json!({
            "version": ctx.build.version,
            "commit":  ctx.build.commit,
            "date":    ctx.build.date,
        });
        let rendered = serde_json::to_string_pretty(&payload)
            .map_err(|e| CliError::Other(e.into()))?;
        writeln!(ctx.io.out(), "{rendered}").map_err(|e| CliError::Other(e.into()))?;
    } else {
        writeln!(
            ctx.io.out(),
            "bb {} (commit {}, built {})",
            ctx.build.version,
            ctx.build.commit,
            ctx.build.date,
        )
        .map_err(|e| CliError::Other(e.into()))?;
    }

    maybe_notice(ctx).await;
    Ok(())
}

async fn maybe_notice(ctx: &mut Context) {
    if !ctx.io.is_stderr_tty() && !ctx.io.is_stdout_tty() {
        return;
    }
    let http = ctx.http_client().clone();
    if let Ok(Some(notice)) = update::check(&http, ctx.build.version).await {
        let icon = ctx.io.cs().warning_icon();
        let _ = writeln!(
            ctx.io.err(),
            "{icon} A newer version of bb is available: {} → {}. {}",
            notice.current,
            notice.latest,
            notice.upgrade_hint,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn prints_version_line() {
        let (mut ctx, bufs) = Context::test();
        run(&mut ctx).await.unwrap();
        let out = bufs.stdout_string();
        assert!(out.starts_with("bb "));
        assert!(out.contains("commit"));
        assert!(out.contains("built"));
    }

    #[tokio::test]
    async fn json_mode_prints_structured_payload() {
        let (mut ctx, bufs) = Context::test();
        run_with(VersionArgs { json: true }, &mut ctx).await.unwrap();
        let out = bufs.stdout_string();
        assert!(out.contains("\"version\""));
        assert!(out.contains("\"commit\""));
        assert!(out.contains("\"date\""));
    }
}
