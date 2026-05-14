//! HTTP debug logging controlled by `BB_DEBUG` / `BB_DEBUG_FILE`.
//!
//! `BB_DEBUG=1|true` → redacts `Authorization`. `BB_DEBUG=api:verbose` → no
//! redaction (useful when you really need to see the token, e.g. to compare
//! against curl). `BB_DEBUG_FILE=<path>` routes the dump to a file — required
//! for MCP stdio mode where stderr is reserved.

use std::io::Write;

use bytes::Bytes;

const BODY_TRUNCATE: usize = 4 * 1024;
const REDACTED: &str = "<redacted>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugMode {
    Off,
    On,
    Verbose,
}

impl DebugMode {
    /// Read `BB_DEBUG` from the environment.
    pub fn from_env() -> Self {
        match std::env::var("BB_DEBUG").ok().as_deref() {
            None | Some("") | Some("0") | Some("false") => DebugMode::Off,
            Some("api:verbose") | Some("verbose") => DebugMode::Verbose,
            _ => DebugMode::On,
        }
    }

    pub fn is_on(self) -> bool {
        !matches!(self, DebugMode::Off)
    }
}

/// Where debug output goes. `BB_DEBUG_FILE` redirects from stderr.
fn open_sink() -> Box<dyn Write + Send> {
    if let Some(path) = std::env::var_os("BB_DEBUG_FILE") {
        if !path.is_empty() {
            if let Ok(file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                return Box::new(file);
            }
        }
    }
    Box::new(std::io::stderr())
}

pub fn log_request(mode: DebugMode, req: &reqwest::Request) {
    if !mode.is_on() {
        return;
    }
    let mut sink = open_sink();
    let _ = writeln!(sink, "> {} {}", req.method(), req.url());
    for (name, value) in req.headers() {
        let printable = if name == reqwest::header::AUTHORIZATION && mode != DebugMode::Verbose {
            REDACTED.to_string()
        } else {
            value.to_str().unwrap_or("<binary>").to_string()
        };
        let _ = writeln!(sink, "> {name}: {printable}");
    }
    if let Some(body) = req.body() {
        if let Some(bytes) = body.as_bytes() {
            let _ = writeln!(sink, "> {}", preview(bytes));
        }
    }
    let _ = writeln!(sink, ">");
}

pub fn log_response(
    mode: DebugMode,
    method: &reqwest::Method,
    url: &reqwest::Url,
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
    body: &Bytes,
) {
    if !mode.is_on() {
        return;
    }
    let mut sink = open_sink();
    let _ = writeln!(sink, "< {} {} → {}", method, url, status);
    for (name, value) in headers {
        let _ = writeln!(
            sink,
            "< {name}: {}",
            value.to_str().unwrap_or("<binary>")
        );
    }
    let _ = writeln!(sink, "< {}", preview(body));
    let _ = writeln!(sink, "<");
}

fn preview(bytes: &[u8]) -> String {
    let truncated = bytes.len() > BODY_TRUNCATE;
    let slice = &bytes[..bytes.len().min(BODY_TRUNCATE)];
    let text = String::from_utf8_lossy(slice);
    if truncated {
        format!("{text}… <truncated, {} bytes total>", bytes.len())
    } else {
        text.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_env() {
        let prev = std::env::var_os("BB_DEBUG");
        std::env::remove_var("BB_DEBUG");
        assert_eq!(DebugMode::from_env(), DebugMode::Off);
        std::env::set_var("BB_DEBUG", "1");
        assert_eq!(DebugMode::from_env(), DebugMode::On);
        std::env::set_var("BB_DEBUG", "api:verbose");
        assert_eq!(DebugMode::from_env(), DebugMode::Verbose);
        std::env::set_var("BB_DEBUG", "0");
        assert_eq!(DebugMode::from_env(), DebugMode::Off);
        match prev {
            Some(v) => std::env::set_var("BB_DEBUG", v),
            None => std::env::remove_var("BB_DEBUG"),
        }
    }
}
