//! TTY-aware columnar output.
//!
//! - **TTY stdout** (`is_stdout_tty == true`): uppercase header rendered in bold
//!   (when color is enabled), columns padded with two-space gutters. Cells in
//!   `truncatable` columns shrink to fit the terminal width.
//! - **Piped stdout** (`is_stdout_tty == false`): tab-separated rows, no header,
//!   no colour. ANSI escape sequences pre-baked into cells get stripped.

use std::io;

use anstream::adapter::strip_str;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::IoStreams;

/// A column descriptor. `wrap = true` opts the column into truncation when the
/// terminal is narrower than the natural column widths.
#[derive(Debug, Clone)]
pub struct Column {
    pub header: String,
    pub wrap: bool,
}

impl Column {
    pub fn new(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            wrap: false,
        }
    }

    /// Mark this column as truncatable when the terminal is too narrow.
    pub fn truncatable(mut self) -> Self {
        self.wrap = true;
        self
    }
}

pub struct TablePrinter<'a> {
    io: &'a mut IoStreams,
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
    /// Override of the terminal width — used by tests so the truncation logic
    /// is reachable without poking at the controlling terminal.
    width_override: Option<usize>,
}

impl<'a> TablePrinter<'a> {
    pub fn new(io: &'a mut IoStreams, columns: Vec<Column>) -> Self {
        Self {
            io,
            columns,
            rows: Vec::new(),
            width_override: None,
        }
    }

    /// Override terminal width detection (tests).
    #[cfg(test)]
    pub fn with_width(mut self, w: usize) -> Self {
        self.width_override = Some(w);
        self
    }

    pub fn add_row<I, S>(&mut self, cells: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.rows.push(cells.into_iter().map(Into::into).collect());
    }

    pub fn render(self) -> io::Result<()> {
        if self.io.is_stdout_tty() {
            self.render_tty()
        } else {
            self.render_tsv()
        }
    }

    fn render_tsv(self) -> io::Result<()> {
        let out = self.io.out();
        for row in &self.rows {
            let cells: Vec<String> = row.iter().map(|c| visible_string(c)).collect();
            writeln!(out, "{}", cells.join("\t"))?;
        }
        Ok(())
    }

    fn render_tty(self) -> io::Result<()> {
        let TablePrinter {
            io,
            columns,
            rows,
            width_override,
        } = self;
        let col_count = columns.len();
        if col_count == 0 {
            return Ok(());
        }

        let cs = io.cs();

        // Natural widths from headers + cells.
        let mut widths = vec![0usize; col_count];
        for (i, col) in columns.iter().enumerate() {
            widths[i] = visible_width(&col.header);
        }
        for row in &rows {
            for (i, cell) in row.iter().enumerate().take(col_count) {
                widths[i] = widths[i].max(visible_width(cell));
            }
        }

        // Shrink truncatable columns if we'd otherwise exceed terminal width.
        let gutter = 2usize;
        let term_width = width_override.or_else(detect_term_width).unwrap_or(usize::MAX);
        let mut total: usize =
            widths.iter().sum::<usize>() + gutter * col_count.saturating_sub(1);
        if total > term_width {
            for i in (0..col_count).rev() {
                if !columns[i].wrap {
                    continue;
                }
                if total <= term_width {
                    break;
                }
                let overflow = total - term_width;
                let new_w = widths[i].saturating_sub(overflow).max(3);
                let saved = widths[i] - new_w;
                widths[i] = new_w;
                total -= saved;
            }
        }

        let out = io.out();

        // Header row.
        for (i, col) in columns.iter().enumerate() {
            let header_text = col.header.to_uppercase();
            let cell = fit_cell(&header_text, widths[i]);
            let styled = cs.bold(&cell);
            let pad = widths[i].saturating_sub(visible_width(&cell));
            if i + 1 == col_count {
                writeln!(out, "{styled}")?;
            } else {
                write!(out, "{styled}{}{}", " ".repeat(pad), " ".repeat(gutter))?;
            }
        }

        // Data rows.
        for row in &rows {
            for (i, w) in widths.iter().enumerate().take(col_count) {
                let raw = row.get(i).map(String::as_str).unwrap_or("");
                let cell = fit_cell(raw, *w);
                let pad = w.saturating_sub(visible_width(&cell));
                if i + 1 == col_count {
                    writeln!(out, "{cell}")?;
                } else {
                    write!(out, "{cell}{}{}", " ".repeat(pad), " ".repeat(gutter))?;
                }
            }
        }
        Ok(())
    }
}

fn detect_term_width() -> Option<usize> {
    use terminal_size::{terminal_size, Width};
    terminal_size().map(|(Width(w), _)| w as usize)
}

fn visible_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for chunk in strip_str(s) {
        out.push_str(chunk);
    }
    out
}

fn visible_width(s: &str) -> usize {
    UnicodeWidthStr::width(visible_string(s).as_str())
}

/// Shrink `s` to at most `max` display cells, appending an ellipsis if cropped.
/// Cells already shorter than `max` are returned as-is (with ANSI preserved).
fn fit_cell(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let plain = visible_string(s);
    if UnicodeWidthStr::width(plain.as_str()) <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".to_string();
    }
    let cap = max - 1;
    let mut width = 0usize;
    let mut out = String::new();
    for ch in plain.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + w > cap {
            break;
        }
        out.push(ch);
        width += w;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(name: &str) -> Column {
        Column::new(name)
    }

    #[test]
    fn non_tty_emits_tab_separated_no_header() {
        let (mut io, bufs) = IoStreams::test();
        let mut t = TablePrinter::new(&mut io, vec![header("number"), header("title")]);
        t.add_row(["1", "Initial commit"]);
        t.add_row(["2", "Fix bug"]);
        t.render().unwrap();
        assert_eq!(bufs.stdout_string(), "1\tInitial commit\n2\tFix bug\n");
    }

    #[test]
    fn non_tty_strips_pre_colored_cells() {
        let (mut io, bufs) = IoStreams::test();
        let mut t = TablePrinter::new(&mut io, vec![header("state")]);
        t.add_row(["\u{1b}[32mopen\u{1b}[0m".to_string()]);
        t.render().unwrap();
        assert_eq!(bufs.stdout_string(), "open\n");
    }

    #[test]
    fn tty_renders_aligned_header_and_rows() {
        let (mut io, bufs) = IoStreams::test();
        io.force_stdout_tty(true);
        let mut t = TablePrinter::new(&mut io, vec![header("number"), header("title")]);
        t.add_row(["1", "Hello"]);
        t.add_row(["234", "Goodbye"]);
        t.render().unwrap();
        let out = bufs.stdout_string();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "NUMBER  TITLE");
        assert_eq!(lines[1], "1       Hello");
        assert_eq!(lines[2], "234     Goodbye");
    }

    #[test]
    fn tty_truncates_wrapped_column() {
        let (mut io, bufs) = IoStreams::test();
        io.force_stdout_tty(true);
        let cols = vec![header("a"), Column::new("title").truncatable()];
        let mut t = TablePrinter::new(&mut io, cols).with_width(12);
        t.add_row(["1", "abcdefghijklmnop"]);
        t.render().unwrap();
        let out = bufs.stdout_string();
        let row = out.lines().nth(1).unwrap();
        // 1 + two spaces gutter + truncated title (9 wide ending in …) = 12 cells.
        assert!(row.starts_with("1  "));
        assert!(row.ends_with('…'));
        assert_eq!(UnicodeWidthStr::width(row), 12);
    }
}
