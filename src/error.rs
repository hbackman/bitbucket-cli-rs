use std::process::ExitCode;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    /// Bad CLI args. clap normally prints usage and exits before we see this;
    /// this variant exists for hand-rolled flag validation. Exit code 2.
    #[error("invalid arguments: {0}")]
    Flag(String),

    /// The error was already printed to stderr; just exit nonzero quietly.
    #[error("(silent)")]
    Silent,

    /// Authentication failed or missing. Exit code 4.
    #[error("authentication: {0}")]
    Auth(String),

    /// Resource not found. Exit code 3.
    #[error("not found: {0}")]
    NotFound(String),

    /// Rate-limited and exhausted our retry budget. Exit code 5.
    #[error("rate limited (retry after {retry_after_secs}s)")]
    RateLimit { retry_after_secs: u64 },

    /// Interactive prompt was cancelled (Ctrl-C). Exit code 6.
    #[error("cancelled")]
    Cancel,

    /// Stub for not-yet-implemented commands.
    #[error("not yet implemented")]
    NotImplemented,

    /// Catch-all wrapping an inner error.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl CliError {
    pub fn exit_code(&self) -> u8 {
        match self {
            CliError::Flag(_) => 2,
            CliError::NotFound(_) => 3,
            CliError::Auth(_) => 4,
            CliError::RateLimit { .. } => 5,
            CliError::Cancel => 6,
            CliError::NotImplemented | CliError::Silent | CliError::Other(_) => 1,
        }
    }
}

/// Prints `err` to stderr (unless it's [`CliError::Silent`]) and returns the exit code.
///
/// Called from `main()` after `cli::run` has consumed the `Context`, so we write
/// directly to `std::io::stderr` rather than going through `IoStreams`. Command code
/// should never call this directly — return the `CliError` and let `main` handle it.
pub fn report(err: CliError) -> ExitCode {
    let code = err.exit_code();
    match err {
        CliError::Silent => {}
        CliError::Flag(msg) => eprintln!("bb: {msg}"),
        CliError::NotFound(msg) => eprintln!("bb: not found: {msg}"),
        CliError::Auth(msg) => eprintln!("bb: authentication: {msg}"),
        CliError::RateLimit { retry_after_secs } => {
            eprintln!("bb: rate limited; retry after {retry_after_secs}s")
        }
        CliError::Cancel => eprintln!("bb: cancelled"),
        CliError::NotImplemented => eprintln!("bb: not yet implemented"),
        CliError::Other(e) => eprintln!("bb: {e:#}"),
    }
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_match_spec() {
        assert_eq!(CliError::Flag("x".into()).exit_code(), 2);
        assert_eq!(CliError::NotFound("x".into()).exit_code(), 3);
        assert_eq!(CliError::Auth("x".into()).exit_code(), 4);
        assert_eq!(CliError::RateLimit { retry_after_secs: 7 }.exit_code(), 5);
        assert_eq!(CliError::Cancel.exit_code(), 6);
        assert_eq!(CliError::NotImplemented.exit_code(), 1);
        assert_eq!(CliError::Silent.exit_code(), 1);
        assert_eq!(CliError::Other(anyhow::anyhow!("x")).exit_code(), 1);
    }
}
