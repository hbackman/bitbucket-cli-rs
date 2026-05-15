//! Tiny stderr spinner for long-running operations.
//!
//! Renders a braille-frame glyph + an optional message on stderr, updated
//! every 100ms. Construct with [`Spinner::start`]; the spinner stops when the
//! returned value is dropped (or via [`Spinner::stop`]). When stderr isn't a
//! TTY the spinner is a no-op so piped/CI output stays quiet.
//!
//! Implementation notes:
//! - The animation runs on a tokio task; `stop()` aborts it and clears the
//!   line we wrote to.
//! - `Drop` falls back to abort+clear in case a caller forgets to stop()
//!   explicitly (e.g. an `?` early-return).

use std::io::Write;
use std::time::Duration;

use tokio::task::JoinHandle;

const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const FRAME_MS: u64 = 100;

pub struct Spinner {
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    /// Start a spinner with the given message. When `stderr_is_tty` is false,
    /// returns an inert spinner that prints nothing.
    pub fn start(msg: impl Into<String>, stderr_is_tty: bool) -> Self {
        if !stderr_is_tty {
            return Self { handle: None };
        }
        let msg = msg.into();
        let handle = tokio::spawn(async move {
            let mut i: usize = 0;
            loop {
                {
                    let mut err = std::io::stderr().lock();
                    let _ = write!(err, "\r{} {}", FRAMES[i % FRAMES.len()], msg);
                    let _ = err.flush();
                }
                tokio::time::sleep(Duration::from_millis(FRAME_MS)).await;
                i += 1;
            }
        });
        Self {
            handle: Some(handle),
        }
    }

    /// Stop the spinner and clear its line. Idempotent.
    pub fn stop(mut self) {
        self.abort_and_clear();
    }

    fn abort_and_clear(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            let mut err = std::io::stderr().lock();
            // CR, clear-to-EOL, flush so the next write starts on a clean line.
            let _ = write!(err, "\r\x1b[K");
            let _ = err.flush();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.abort_and_clear();
    }
}
