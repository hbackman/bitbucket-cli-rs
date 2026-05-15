//! Text helpers — pluralize, truncate, indent, and (relative/absolute) time
//! formatters used by table cells and command output.

use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime, UtcOffset};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// English pluralization. `n == 1` keeps the singular; everything else gets `s`.
pub fn pluralize(n: i64, singular: &str) -> String {
    if n == 1 || n == -1 {
        format!("{n} {singular}")
    } else {
        format!("{n} {singular}s")
    }
}

/// Truncate `s` so its visible (display) width is at most `max` cells. If
/// truncation happens, the result ends in `…` (U+2026, width 1).
///
/// Multi-byte safe via `unicode-width`.
pub fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(s) <= max {
        return s.to_string();
    }
    const ELLIPSIS: char = '…';
    let ell_w = UnicodeWidthChar::width(ELLIPSIS).unwrap_or(1);
    if max <= ell_w {
        return ELLIPSIS.to_string();
    }
    let cap = max - ell_w;
    let mut width = 0usize;
    let mut out = String::new();
    for ch in s.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + w > cap {
            break;
        }
        out.push(ch);
        width += w;
    }
    out.push(ELLIPSIS);
    out
}

/// Prefix every line of `s` with `prefix`. Empty lines stay empty.
pub fn indent(s: &str, prefix: &str) -> String {
    let mut out = String::new();
    for (i, line) in s.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if !line.is_empty() {
            out.push_str(prefix);
        }
        out.push_str(line);
    }
    out
}

/// `gh`-style relative time — `"about 2 hours ago"`, `"3 days ago"`.
pub fn rel_time(t: OffsetDateTime) -> String {
    let now = OffsetDateTime::now_utc();
    rel_time_from(t, now)
}

fn rel_time_from(t: OffsetDateTime, now: OffsetDateTime) -> String {
    let diff: Duration = now - t;
    let secs_signed = diff.whole_seconds();
    let (suffix, secs) = if secs_signed >= 0 {
        ("ago", secs_signed as u64)
    } else {
        ("from now", (-secs_signed) as u64)
    };
    let phrase = match secs {
        0..=59 => "less than a minute".to_string(),
        60..=119 => "about 1 minute".to_string(),
        120..=3599 => format!("about {} minutes", secs / 60),
        3600..=7199 => "about 1 hour".to_string(),
        7200..=86_399 => format!("about {} hours", secs / 3600),
        86_400..=172_799 => "1 day".to_string(),
        172_800..=2_591_999 => format!("{} days", secs / 86_400),
        2_592_000..=5_183_999 => "about 1 month".to_string(),
        5_184_000..=31_535_999 => format!("about {} months", secs / 2_592_000),
        31_536_000..=63_071_999 => "about 1 year".to_string(),
        _ => format!("about {} years", secs / 31_536_000),
    };
    format!("{phrase} {suffix}")
}

/// RFC 3339 timestamp in UTC. Falls back to the `Debug` form if formatting fails.
pub fn abs_time(t: OffsetDateTime) -> String {
    t.to_offset(UtcOffset::UTC)
        .format(&Rfc3339)
        .unwrap_or_else(|_| format!("{t:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn pluralize_singular_vs_plural() {
        assert_eq!(pluralize(0, "commit"), "0 commits");
        assert_eq!(pluralize(1, "commit"), "1 commit");
        assert_eq!(pluralize(5, "commit"), "5 commits");
        assert_eq!(pluralize(-1, "commit"), "-1 commit");
    }

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string_gets_ellipsis() {
        let out = truncate("hello world", 8);
        assert_eq!(UnicodeWidthStr::width(out.as_str()), 8);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn truncate_multibyte_respects_display_width() {
        // "中文" is 4 display cells. Truncate to 3 → "中…" (2 + 1 = 3).
        let out = truncate("中文字幕", 3);
        assert_eq!(out, "中…");
        assert_eq!(UnicodeWidthStr::width(out.as_str()), 3);
    }

    #[test]
    fn truncate_empty_max() {
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn indent_prefixes_each_line() {
        assert_eq!(indent("a\nb\nc", "> "), "> a\n> b\n> c");
        assert_eq!(indent("a\n\nb", "> "), "> a\n\n> b");
    }

    #[test]
    fn rel_time_buckets_match_spec() {
        let now = datetime!(2026-05-14 12:00:00 UTC);
        let cases = [
            (30, "less than a minute ago"),
            (5 * 60, "about 5 minutes ago"),
            (2 * 3600, "about 2 hours ago"),
            (3 * 86_400, "3 days ago"),
            (4 * 7 * 86_400, "28 days ago"),
            (6 * 30 * 86_400, "about 6 months ago"),
            (2 * 365 * 86_400, "about 2 years ago"),
        ];
        for (secs, expected) in cases {
            let t = now - Duration::seconds(secs as i64);
            assert_eq!(rel_time_from(t, now), expected, "secs = {secs}");
        }
    }

    #[test]
    fn rel_time_handles_future() {
        let now = datetime!(2026-05-14 12:00:00 UTC);
        let t = now + Duration::seconds(5 * 60);
        assert_eq!(rel_time_from(t, now), "about 5 minutes from now");
    }

    #[test]
    fn abs_time_is_rfc3339_utc() {
        let t = datetime!(2026-05-14 13:00:00 UTC);
        assert_eq!(abs_time(t), "2026-05-14T13:00:00Z");
    }
}
