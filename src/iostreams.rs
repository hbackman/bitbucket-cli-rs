use std::io::{self, IsTerminal, Read, Write};
use std::sync::{Arc, Mutex};

use owo_colors::OwoColorize;

pub mod spinner;
pub mod table;

pub use table::{Column, TablePrinter};

pub struct IoStreams {
    stdin: Box<dyn Read + Send>,
    stdout: Box<dyn Write + Send>,
    stderr: Box<dyn Write + Send>,
    color_enabled: bool,
    is_stdout_tty: bool,
    is_stderr_tty: bool,
    is_stdin_tty: bool,
}

impl IoStreams {
    pub fn system() -> Self {
        let is_stdout_tty = io::stdout().is_terminal();
        let is_stderr_tty = io::stderr().is_terminal();
        let is_stdin_tty = io::stdin().is_terminal();
        Self {
            stdin: Box::new(io::stdin()),
            stdout: Box::new(io::stdout()),
            stderr: Box::new(io::stderr()),
            color_enabled: detect_color(is_stdout_tty),
            is_stdout_tty,
            is_stderr_tty,
            is_stdin_tty,
        }
    }

    pub fn test() -> (Self, TestBuffers) {
        let stdin = Arc::new(Mutex::new(Vec::<u8>::new()));
        let stdout = Arc::new(Mutex::new(Vec::<u8>::new()));
        let stderr = Arc::new(Mutex::new(Vec::<u8>::new()));
        let bufs = TestBuffers {
            stdin: stdin.clone(),
            stdout: stdout.clone(),
            stderr: stderr.clone(),
        };
        let io = Self {
            stdin: Box::new(SharedReader(stdin)),
            stdout: Box::new(SharedWriter(stdout)),
            stderr: Box::new(SharedWriter(stderr)),
            color_enabled: false,
            is_stdout_tty: false,
            is_stderr_tty: false,
            is_stdin_tty: false,
        };
        (io, bufs)
    }

    pub fn out(&mut self) -> &mut dyn Write {
        &mut *self.stdout
    }

    pub fn err(&mut self) -> &mut dyn Write {
        &mut *self.stderr
    }

    pub fn input(&mut self) -> &mut dyn Read {
        &mut *self.stdin
    }

    pub fn color_enabled(&self) -> bool {
        self.color_enabled
    }

    pub fn is_stdout_tty(&self) -> bool {
        self.is_stdout_tty
    }

    pub fn is_stderr_tty(&self) -> bool {
        self.is_stderr_tty
    }

    pub fn is_stdin_tty(&self) -> bool {
        self.is_stdin_tty
    }

    /// Borrowed view of the active color scheme. Cheap to copy.
    pub fn cs(&self) -> ColorScheme {
        ColorScheme {
            enabled: self.color_enabled,
        }
    }

    /// Test helper — marks stdout as a TTY without giving up the in-memory buffer.
    /// Lets renderers exercise their TTY path under `cargo test`.
    #[cfg(test)]
    pub fn force_stdout_tty(&mut self, tty: bool) {
        self.is_stdout_tty = tty;
    }

    /// Test helper — toggles color independently of TTY detection.
    #[cfg(test)]
    pub fn force_color(&mut self, on: bool) {
        self.color_enabled = on;
    }

    /// Pager support is post-MVP; left as a no-op so call sites compile.
    pub fn start_pager(&mut self) -> io::Result<()> {
        Ok(())
    }

    pub fn stop_pager(&mut self) {}
}

#[derive(Clone)]
pub struct TestBuffers {
    pub stdin: Arc<Mutex<Vec<u8>>>,
    pub stdout: Arc<Mutex<Vec<u8>>>,
    pub stderr: Arc<Mutex<Vec<u8>>>,
}

impl TestBuffers {
    pub fn stdout_string(&self) -> String {
        String::from_utf8(self.stdout.lock().unwrap().clone()).expect("stdout was not utf-8")
    }

    pub fn stderr_string(&self) -> String {
        String::from_utf8(self.stderr.lock().unwrap().clone()).expect("stderr was not utf-8")
    }
}

struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct SharedReader(Arc<Mutex<Vec<u8>>>);

impl Read for SharedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut guard = self.0.lock().unwrap();
        let n = buf.len().min(guard.len());
        buf[..n].copy_from_slice(&guard[..n]);
        guard.drain(..n);
        Ok(n)
    }
}

fn detect_color(is_stdout_tty: bool) -> bool {
    // CLICOLOR_FORCE wins outright (per https://bixense.com/clicolors).
    if std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0") {
        return true;
    }
    // NO_COLOR (https://no-color.org) disables color when set to any value.
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var_os("CLICOLOR").is_some_and(|v| v == "0") {
        return false;
    }
    is_stdout_tty
}

/// Thin facade over `owo-colors` that respects the active color setting.
///
/// Methods return owned `String`s and pass input through unchanged when color is
/// disabled, which means callers never have to branch on `color_enabled` themselves.
#[derive(Debug, Clone, Copy)]
pub struct ColorScheme {
    enabled: bool,
}

impl ColorScheme {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn red<S: AsRef<str>>(&self, s: S) -> String {
        if self.enabled {
            format!("{}", s.as_ref().red())
        } else {
            s.as_ref().to_string()
        }
    }

    pub fn green<S: AsRef<str>>(&self, s: S) -> String {
        if self.enabled {
            format!("{}", s.as_ref().green())
        } else {
            s.as_ref().to_string()
        }
    }

    pub fn yellow<S: AsRef<str>>(&self, s: S) -> String {
        if self.enabled {
            format!("{}", s.as_ref().yellow())
        } else {
            s.as_ref().to_string()
        }
    }

    pub fn cyan<S: AsRef<str>>(&self, s: S) -> String {
        if self.enabled {
            format!("{}", s.as_ref().cyan())
        } else {
            s.as_ref().to_string()
        }
    }

    pub fn gray<S: AsRef<str>>(&self, s: S) -> String {
        if self.enabled {
            format!("{}", s.as_ref().bright_black())
        } else {
            s.as_ref().to_string()
        }
    }

    pub fn magenta<S: AsRef<str>>(&self, s: S) -> String {
        if self.enabled {
            format!("{}", s.as_ref().magenta())
        } else {
            s.as_ref().to_string()
        }
    }

    pub fn blue<S: AsRef<str>>(&self, s: S) -> String {
        if self.enabled {
            format!("{}", s.as_ref().blue())
        } else {
            s.as_ref().to_string()
        }
    }

    pub fn bold<S: AsRef<str>>(&self, s: S) -> String {
        if self.enabled {
            format!("{}", s.as_ref().bold())
        } else {
            s.as_ref().to_string()
        }
    }

    pub fn success_icon(&self) -> String {
        let icon = if use_unicode(self.enabled) {
            "✓"
        } else {
            "v"
        };
        self.green(icon)
    }

    pub fn failure_icon(&self) -> String {
        let icon = if use_unicode(self.enabled) {
            "✗"
        } else {
            "x"
        };
        self.red(icon)
    }

    pub fn warning_icon(&self) -> String {
        self.yellow("!")
    }

    pub fn neutral_icon(&self) -> String {
        "-".to_string()
    }

    /// Wrap `label` in an OSC 8 hyperlink escape so terminals that support it
    /// render the label as a clickable link to `url`. Falls back to plain
    /// `label` (or `label (url)` for non-color/non-tty output) so users who
    /// can't click still see the URL.
    ///
    /// Emit only when color is enabled — that's our proxy for "this is a real
    /// terminal that probably supports OSC 8". Terminals that don't recognize
    /// the escape silently ignore it, but emitting it into a pipe or a dumb
    /// terminal would just look like garbage.
    pub fn hyperlink<L: AsRef<str>, U: AsRef<str>>(&self, label: L, url: U) -> String {
        let label = label.as_ref();
        let url = url.as_ref();
        if self.enabled {
            format!("\x1b]8;;{url}\x1b\\{label}\x1b]8;;\x1b\\")
        } else if label == url {
            label.to_string()
        } else {
            format!("{label} ({url})")
        }
    }
}

fn use_unicode(color_enabled: bool) -> bool {
    if color_enabled {
        return true;
    }
    for var in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            let v = v.to_lowercase();
            if v.contains("utf-8") || v.contains("utf8") {
                return true;
            }
            if !v.is_empty() {
                return false;
            }
        }
    }
    // No locale env set — assume UTF-8 is fine on modern systems.
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streams_capture_writes() {
        let (mut io, bufs) = IoStreams::test();
        write!(io.out(), "hello").unwrap();
        write!(io.err(), "warning").unwrap();
        assert_eq!(bufs.stdout_string(), "hello");
        assert_eq!(bufs.stderr_string(), "warning");
    }

    #[test]
    fn test_streams_have_color_disabled() {
        let (io, _) = IoStreams::test();
        assert!(!io.color_enabled());
        assert!(!io.is_stdout_tty());
    }

    #[test]
    fn test_streams_read_from_stdin_buffer() {
        let (mut io, bufs) = IoStreams::test();
        bufs.stdin.lock().unwrap().extend_from_slice(b"input bytes");
        let mut out = String::new();
        io.input().read_to_string(&mut out).unwrap();
        assert_eq!(out, "input bytes");
    }

    #[test]
    fn color_scheme_passthrough_when_disabled() {
        let cs = ColorScheme::new(false);
        assert_eq!(cs.red("hi"), "hi");
        assert_eq!(cs.green("hi"), "hi");
        assert_eq!(cs.bold("hi"), "hi");
    }

    #[test]
    fn color_scheme_wraps_when_enabled() {
        let cs = ColorScheme::new(true);
        let red = cs.red("hi");
        assert!(red.contains("\u{1b}["), "expected ANSI escape, got {red:?}");
        assert!(red.contains("hi"));
    }
}
