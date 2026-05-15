//! Conventional status messages.
//!
//! Everything here writes to `io.err()` — these are status / progress / warning
//! lines, not command output, and must not pollute stdout when the caller is
//! piping into another process.

use std::io;

use crate::iostreams::IoStreams;

/// Neutral status line: `- <msg>` to stderr.
pub fn print_notice(io: &mut IoStreams, msg: &str) -> io::Result<()> {
    let icon = io.cs().neutral_icon();
    writeln!(io.err(), "{icon} {msg}")
}

/// Success line: `✓ <msg>` (green check on TTY) to stderr.
pub fn print_success(io: &mut IoStreams, msg: &str) -> io::Result<()> {
    let icon = io.cs().success_icon();
    writeln!(io.err(), "{icon} {msg}")
}

/// Soft warning that doesn't fail the command: `! <msg>` (yellow) to stderr.
pub fn print_warning(io: &mut IoStreams, msg: &str) -> io::Result<()> {
    let icon = io.cs().warning_icon();
    writeln!(io.err(), "{icon} {msg}")
}

/// Hard error: `! <msg>` (red) to stderr. Use right before returning a `CliError`.
pub fn print_error(io: &mut IoStreams, msg: &str) -> io::Result<()> {
    let icon = io.cs().failure_icon();
    writeln!(io.err(), "{icon} {msg}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iostreams::IoStreams;

    #[test]
    fn success_writes_to_stderr_not_stdout() {
        let (mut io, bufs) = IoStreams::test();
        print_success(&mut io, "logged in").unwrap();
        assert_eq!(bufs.stdout_string(), "");
        let err = bufs.stderr_string();
        assert!(err.contains("logged in"));
    }

    #[test]
    fn notice_uses_neutral_icon() {
        let (mut io, bufs) = IoStreams::test();
        print_notice(&mut io, "opening browser").unwrap();
        assert_eq!(bufs.stderr_string(), "- opening browser\n");
    }
}
