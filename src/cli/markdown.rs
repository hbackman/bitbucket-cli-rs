//! Markdown body renderer used by `bbk pr view` and `bbk repo view`.
//!
//! Tries an external renderer in order (glow, mdcat, bat --language=markdown)
//! when color is enabled and `BBK_MARKDOWN_RENDERER=plain` isn't set; falls
//! back to a minimal inline renderer that just bolds `#` headings and indents
//! each line by two spaces.
//!
//! The external-renderer output is indented to two spaces to match the
//! surrounding terminal layout. On any failure (binary not found, non-zero
//! exit, write error) we silently degrade to the inline renderer so the user
//! still sees the body.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::iostreams::ColorScheme;

/// Render `body` for display under the current ColorScheme. Returns the
/// rendered text without a trailing newline so callers can decide on spacing.
pub fn render(body: &str, cs: &ColorScheme) -> String {
    if cs.enabled() && !plain_forced() {
        if let Some(rendered) = render_external(body) {
            return indent_two_spaces(&rendered);
        }
    }
    render_inline(body, cs)
}

fn plain_forced() -> bool {
    std::env::var("BBK_MARKDOWN_RENDERER").as_deref() == Ok("plain")
}

/// Try each external renderer in priority order. Returns `Some(stdout)` from
/// the first one that runs cleanly, or `None` if none are available.
fn render_external(body: &str) -> Option<String> {
    for (cmd, args) in EXTERNAL_RENDERERS {
        if let Some(out) = try_renderer(cmd, args, body) {
            return Some(out);
        }
    }
    None
}

/// (program, extra args). Each program reads markdown on stdin and writes
/// rendered output on stdout. We're capturing stdout via a pipe, so we have to
/// override each tool's "auto" color detection (which would otherwise see the
/// non-TTY stdout and produce plain text). `glow -s dark` forces the dark
/// theme; `bat --color=always` forces ANSI even when piped; `mdcat` doesn't
/// have a force-color flag but respects CLICOLOR_FORCE / NO_COLOR upstream of
/// it, so it falls through gracefully if env says no.
const EXTERNAL_RENDERERS: &[(&str, &[&str])] = &[
    ("glow", &["-s", "dark"]),
    ("mdcat", &["--columns", "80"]),
    (
        "bat",
        &[
            "--language=markdown",
            "--style=plain",
            "--paging=never",
            "--color=always",
        ],
    ),
];

fn try_renderer(program: &str, args: &[&str], body: &str) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    {
        let stdin = child.stdin.as_mut()?;
        stdin.write_all(body.as_bytes()).ok()?;
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Indent every line by two spaces so the rendered body sits inside the
/// command's overall left margin. Trailing blank lines from the renderer get
/// trimmed so callers can append their own spacing.
fn indent_two_spaces(s: &str) -> String {
    let trimmed = s.trim_end_matches('\n');
    let mut out = String::with_capacity(trimmed.len() + trimmed.len() / 40);
    for (i, line) in trimmed.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if !line.is_empty() {
            out.push_str("  ");
        }
        out.push_str(line);
    }
    out
}

/// Minimal inline renderer used when no external tool is available (or when
/// color/output is disabled). Bolds `#`/`##` headings, indents every line by
/// two spaces, leaves the rest verbatim so users can still read the markdown.
fn render_inline(body: &str, cs: &ColorScheme) -> String {
    let mut out = String::with_capacity(body.len() + body.len() / 8);
    for line in body.split('\n') {
        out.push_str("  ");
        if let Some(stripped) = heading_text(line) {
            out.push_str(&cs.bold(stripped));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

fn heading_text(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let depth = trimmed.bytes().take_while(|b| *b == b'#').count();
    if depth == 0 || depth > 6 {
        return None;
    }
    let rest = &trimmed[depth..];
    rest.strip_prefix(' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_indents_each_line() {
        let cs = ColorScheme::new(false);
        let out = render_inline("hello\nworld", &cs);
        assert_eq!(out, "  hello\n  world");
    }

    #[test]
    fn inline_bolds_headings_when_color_enabled() {
        let cs = ColorScheme::new(true);
        let out = render_inline("## Summary\nbody", &cs);
        assert!(out.contains("Summary"));
        assert!(out.contains("body"));
        assert!(out.contains("\u{1b}["), "expected ANSI escape, got {out:?}");
    }

    #[test]
    fn heading_recognized() {
        assert_eq!(heading_text("# Title"), Some("Title"));
        assert_eq!(heading_text("##  Sub"), Some(" Sub"));
        assert_eq!(heading_text("plain"), None);
        assert_eq!(heading_text("#######too deep"), None);
    }

    #[test]
    fn render_falls_back_when_color_disabled() {
        // No color => use inline renderer. Don't spawn glow. Headings get
        // their `#` stripped and printed without bold styling.
        let cs = ColorScheme::new(false);
        let out = render("# Hello\nworld", &cs);
        assert_eq!(out, "  Hello\n  world");
    }

    #[test]
    fn indent_two_spaces_handles_blank_lines() {
        let out = indent_two_spaces("a\n\nb\n");
        assert_eq!(out, "  a\n\n  b");
    }
}
