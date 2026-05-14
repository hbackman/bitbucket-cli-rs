//! Implementation of the git-credential helper protocol — see
//! <https://git-scm.com/docs/git-credential>.
//!
//! Git invokes `bb auth git-credential <op>` over stdin/stdout with
//! `key=value` lines, terminated by a blank line.

use std::collections::HashMap;

use anyhow::{anyhow, Result};

/// Operations git might invoke. `store` and `erase` are no-ops — bb owns its own
/// credential store and ignores git's cache notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialOp {
    Get,
    Store,
    Erase,
}

impl CredentialOp {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "get" => Ok(Self::Get),
            "store" => Ok(Self::Store),
            "erase" => Ok(Self::Erase),
            other => Err(anyhow!("unknown git-credential op {other:?}")),
        }
    }
}

/// Parse the `key=value` stream git sends on stdin.
pub fn parse_input(input: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in input.lines() {
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once('=') {
            out.insert(k.to_string(), v.to_string());
        }
    }
    out
}

/// Build the response git expects when we have a credential.
pub fn format_response(host: &str, username: &str, password: &str, protocol: &str) -> String {
    let mut buf = String::new();
    if !protocol.is_empty() {
        buf.push_str(&format!("protocol={protocol}\n"));
    }
    buf.push_str(&format!("host={host}\n"));
    buf.push_str(&format!("username={username}\n"));
    buf.push_str(&format!("password={password}\n"));
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_input_terminates_on_blank_line() {
        let input = "protocol=https\nhost=bitbucket.org\n\nignored=value\n";
        let m = parse_input(input);
        assert_eq!(m.get("protocol").map(String::as_str), Some("https"));
        assert_eq!(m.get("host").map(String::as_str), Some("bitbucket.org"));
        assert!(!m.contains_key("ignored"));
    }

    #[test]
    fn parse_input_handles_missing_blank_line() {
        let input = "protocol=https\nhost=bitbucket.org\n";
        let m = parse_input(input);
        assert_eq!(m.get("host").map(String::as_str), Some("bitbucket.org"));
    }

    #[test]
    fn format_response_includes_all_fields() {
        let r = format_response("bitbucket.org", "x-token-auth", "secret", "https");
        assert!(r.contains("protocol=https"));
        assert!(r.contains("host=bitbucket.org"));
        assert!(r.contains("username=x-token-auth"));
        assert!(r.contains("password=secret"));
    }

    #[test]
    fn op_parses() {
        assert_eq!(CredentialOp::parse("get").unwrap(), CredentialOp::Get);
        assert_eq!(CredentialOp::parse("store").unwrap(), CredentialOp::Store);
        assert_eq!(CredentialOp::parse("erase").unwrap(), CredentialOp::Erase);
        assert!(CredentialOp::parse("bogus").is_err());
    }
}
