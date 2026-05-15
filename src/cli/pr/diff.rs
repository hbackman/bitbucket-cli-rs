//! `bb pr diff [N]` — print the diff for a pull request.
//!
//! Streams the response body straight through `IoStreams::out()` so megabyte-sized
//! diffs don't get buffered. When stdout is a TTY we apply basic per-line ANSI
//! coloring (green +, red -, cyan @@). External diff prettifiers (`delta`,
//! `diff-so-fancy`) on PATH are post-MVP — wire-up point left as a comment.

use clap::Args as ClapArgs;
use futures::StreamExt;

use crate::context::Context;
use crate::error::CliError;
use crate::iostreams::ColorScheme;

use super::display::pr_html_url;
use super::finder;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Pull request number. Defaults to the PR for the current branch.
    pub number: Option<u32>,

    /// Open the diff page in the browser instead.
    #[arg(short = 'w', long)]
    pub web: bool,

    /// Color control: auto (default), always, never.
    #[arg(long, value_name = "WHEN", default_value = "auto")]
    pub color: String,
}

pub async fn run(args: Args, ctx: &mut Context) -> Result<(), CliError> {
    let (repo, pr) = finder::find(ctx, args.number).await?;
    if args.web {
        let url = pr_html_url(&pr).unwrap_or_else(|| {
            format!(
                "https://{}/{}/{}/pull-requests/{}/diff",
                repo.host, repo.workspace, repo.slug, pr.id
            )
        });
        return ctx.browser().open(&url).map_err(CliError::Other);
    }

    let client = ctx.api().await?.clone();
    let cs = match args.color.as_str() {
        "always" => ColorScheme::new(true),
        "never" => ColorScheme::new(false),
        _ => ctx.io.cs(),
    };

    ctx.io.start_pager().map_err(io_err)?;
    let response = client.pull_requests().diff(&repo, pr.id).await?;
    let mut stream = response.bytes_stream();
    let mut buf = Vec::<u8>::new();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| CliError::Other(e.into()))?;
        buf.extend_from_slice(&bytes);
        while let Some(idx) = buf.iter().position(|b| *b == b'\n') {
            let line = buf.drain(..=idx).collect::<Vec<u8>>();
            write_line(ctx, &cs, &line)?;
        }
    }
    if !buf.is_empty() {
        write_line(ctx, &cs, &buf)?;
    }
    ctx.io.stop_pager();
    Ok(())
}

fn write_line(ctx: &mut Context, cs: &ColorScheme, line: &[u8]) -> Result<(), CliError> {
    let s = String::from_utf8_lossy(line);
    let styled = colorize_line(&s, cs);
    write!(ctx.io.out(), "{styled}").map_err(io_err)
}

fn colorize_line(line: &str, cs: &ColorScheme) -> String {
    if !cs.enabled() {
        return line.to_string();
    }
    let trimmed = line.trim_end_matches('\n');
    let suffix: &str = if line.ends_with('\n') { "\n" } else { "" };
    if trimmed.starts_with("+++") || trimmed.starts_with("---") {
        return format!("{}{suffix}", cs.bold(trimmed));
    }
    if trimmed.starts_with("@@") {
        return format!("{}{suffix}", cs.cyan(trimmed));
    }
    if trimmed.starts_with('+') {
        return format!("{}{suffix}", cs.green(trimmed));
    }
    if trimmed.starts_with('-') {
        return format!("{}{suffix}", cs.red(trimmed));
    }
    if trimmed.starts_with("diff ") || trimmed.starts_with("index ") {
        return format!("{}{suffix}", cs.bold(trimmed));
    }
    line.to_string()
}

fn io_err(e: std::io::Error) -> CliError {
    CliError::Other(e.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorize_passthrough_when_disabled() {
        let cs = ColorScheme::new(false);
        assert_eq!(colorize_line("+ added\n", &cs), "+ added\n");
        assert_eq!(colorize_line("@@ hunk @@", &cs), "@@ hunk @@");
    }

    #[test]
    fn colorize_marks_additions_when_enabled() {
        let cs = ColorScheme::new(true);
        let out = colorize_line("+ added\n", &cs);
        assert!(out.contains("\u{1b}["));
        assert!(out.contains("added"));
        assert!(out.ends_with('\n'));
    }
}
