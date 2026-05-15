//! Minimal markdown-ish body renderer for `bbk pr view`.
//!
//! Full markdown rendering is out of scope for MVP; spec 06 calls for a swap-out
//! helper. This emits the body indented by two spaces, bolds `#` / `##` heading
//! lines, and leaves the rest alone so users can still see the markdown
//! verbatim. The interface (`render(s, &cs) -> String`) matches the spec so a
//! richer renderer can drop in later without touching call sites.

use crate::iostreams::ColorScheme;

/// Render `body` with light styling and a two-space indent on every line.
pub fn render(body: &str, cs: &ColorScheme) -> String {
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
    // Drop the trailing newline we added on the last split chunk.
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
    fn indents_each_line() {
        let cs = ColorScheme::new(false);
        let out = render("hello\nworld", &cs);
        assert_eq!(out, "  hello\n  world");
    }

    #[test]
    fn bolds_headings_when_color_enabled() {
        let cs = ColorScheme::new(true);
        let out = render("## Summary\nbody", &cs);
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
}
