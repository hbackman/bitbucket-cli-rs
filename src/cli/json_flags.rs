//! Shared `--json` / `--jq` flag plumbing.
//!
//! Every list/view command attaches [`JsonFlags`] via clap's `#[command(flatten)]`,
//! then calls [`JsonFlags::validate`] to turn the raw flag values into a [`JsonMode`].
//!
//! - `--json` with no value lists the available fields and exits 1 (a
//!   discoverability feature, not a failure).
//! - `--json a,b,c` validates that every field is in the command's `JSON_FIELDS`
//!   list.
//! - `--jq EXPR` requires `--json`; the filter is applied after field projection.

use clap::Args;

use crate::error::CliError;

#[derive(Args, Debug, Default, Clone)]
pub struct JsonFlags {
    /// Comma-separated list of fields to output as JSON. Use `--json` with no
    /// value to list available fields.
    #[arg(
        long,
        value_name = "FIELDS",
        value_delimiter = ',',
        num_args = 0..,
    )]
    pub json: Option<Vec<String>>,

    /// Filter JSON output with a jq expression.
    #[arg(long, value_name = "EXPR")]
    pub jq: Option<String>,

    /// Format JSON output with a Tera template (post-MVP — accepted but unused).
    #[arg(long, value_name = "EXPR", hide = true)]
    pub template: Option<String>,
}

/// Resolved JSON output mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonMode {
    /// No `--json` flag — render the table/TSV path.
    Off,
    /// `--json a,b,c` — project these fields then emit JSON.
    Fields(Vec<String>),
    /// `--json a,b,c --jq EXPR` — project, then pipe through jq.
    FilterFields { fields: Vec<String>, jq: String },
}

impl JsonFlags {
    /// Validate against the command's known fields and resolve a [`JsonMode`].
    ///
    /// Errors:
    /// - `--json` with no value → [`CliError::Flag`] listing `available` (exit 2,
    ///   the discoverability behavior the spec calls out).
    /// - Unknown field name in `--json` → [`CliError::Flag`].
    /// - `--jq` without `--json` → [`CliError::Flag`].
    pub fn validate(&self, available: &[&str]) -> Result<JsonMode, CliError> {
        if self.template.is_some() {
            return Err(CliError::Flag(
                "--template is not yet supported. Use --json + --jq for now.".into(),
            ));
        }

        match (&self.json, &self.jq) {
            (None, None) => Ok(JsonMode::Off),
            (None, Some(_)) => Err(CliError::Flag("--jq requires --json".into())),
            (Some(fields), jq) => {
                if fields.is_empty() {
                    return Err(CliError::Flag(format!(
                        "Specify one or more comma-separated fields for --json.\n\nAvailable fields:\n  {}",
                        available.join("\n  "),
                    )));
                }
                for f in fields {
                    if !available.iter().any(|a| *a == f) {
                        return Err(CliError::Flag(format!(
                            "unknown JSON field {f:?}. Available: {}",
                            available.join(", "),
                        )));
                    }
                }
                match jq {
                    Some(expr) => Ok(JsonMode::FilterFields {
                        fields: fields.clone(),
                        jq: expr.clone(),
                    }),
                    None => Ok(JsonMode::Fields(fields.clone())),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIELDS: &[&str] = &["number", "title", "author"];

    #[test]
    fn off_when_no_flag() {
        let f = JsonFlags::default();
        assert_eq!(f.validate(FIELDS).unwrap(), JsonMode::Off);
    }

    #[test]
    fn empty_json_lists_available_fields_and_errors() {
        let f = JsonFlags {
            json: Some(vec![]),
            ..Default::default()
        };
        let err = f.validate(FIELDS).unwrap_err();
        match err {
            CliError::Flag(msg) => {
                assert!(msg.contains("Available fields"));
                assert!(msg.contains("number"));
                assert!(msg.contains("title"));
            }
            other => panic!("expected Flag, got {other:?}"),
        }
    }

    #[test]
    fn known_fields_pass_through() {
        let f = JsonFlags {
            json: Some(vec!["number".into(), "title".into()]),
            ..Default::default()
        };
        assert_eq!(
            f.validate(FIELDS).unwrap(),
            JsonMode::Fields(vec!["number".into(), "title".into()])
        );
    }

    #[test]
    fn unknown_field_errors() {
        let f = JsonFlags {
            json: Some(vec!["number".into(), "bogus".into()]),
            ..Default::default()
        };
        let err = f.validate(FIELDS).unwrap_err();
        assert!(matches!(err, CliError::Flag(msg) if msg.contains("bogus")));
    }

    #[test]
    fn jq_requires_json() {
        let f = JsonFlags {
            jq: Some(".[].title".into()),
            ..Default::default()
        };
        let err = f.validate(FIELDS).unwrap_err();
        assert!(matches!(err, CliError::Flag(msg) if msg.contains("--jq requires --json")));
    }

    #[test]
    fn jq_with_fields_resolves_to_filter() {
        let f = JsonFlags {
            json: Some(vec!["title".into()]),
            jq: Some(".[].title".into()),
            ..Default::default()
        };
        assert_eq!(
            f.validate(FIELDS).unwrap(),
            JsonMode::FilterFields {
                fields: vec!["title".into()],
                jq: ".[].title".into()
            }
        );
    }

    #[test]
    fn template_rejected_for_now() {
        let f = JsonFlags {
            template: Some("hi".into()),
            ..Default::default()
        };
        assert!(matches!(f.validate(FIELDS), Err(CliError::Flag(_))));
    }
}
