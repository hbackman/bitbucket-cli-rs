//! Shared rendering helpers for `bb pr …`: JSON projection, state colors, URLs.

use serde_json::{json, Value};

use crate::api::types::{Actor, PullRequest};
use crate::iostreams::ColorScheme;

/// `--json` fields supported by `bb pr list` / `bb pr view`.
pub const JSON_FIELDS: &[&str] = &[
    "id",
    "title",
    "body",
    "state",
    "author",
    "sourceBranch",
    "destinationBranch",
    "createdOn",
    "updatedOn",
    "url",
    "draft",
    "reviewers",
    "closeSourceBranch",
    "commentCount",
];

/// Project a [`PullRequest`] into a JSON map keyed by user-facing field names.
pub fn project_pr(pr: &PullRequest, fields: &[String]) -> Value {
    let mut obj = serde_json::Map::new();
    for field in fields {
        let v = match field.as_str() {
            "id" => json!(pr.id),
            "title" => json!(pr.title),
            "body" => pr
                .description
                .as_ref()
                .map(|s| json!(s))
                .unwrap_or(Value::Null),
            "state" => pr.state.as_ref().map(|s| json!(s)).unwrap_or(Value::Null),
            "author" => actor_value(pr.author.as_ref()),
            "sourceBranch" => match source_branch(pr) {
                Some(s) => json!(s),
                None => Value::Null,
            },
            "destinationBranch" => match destination_branch(pr) {
                Some(s) => json!(s),
                None => Value::Null,
            },
            "createdOn" => pr
                .created_on
                .as_ref()
                .map(|s| json!(s))
                .unwrap_or(Value::Null),
            "updatedOn" => pr
                .updated_on
                .as_ref()
                .map(|s| json!(s))
                .unwrap_or(Value::Null),
            "url" => pr_html_url(pr).map(|s| json!(s)).unwrap_or(Value::Null),
            "draft" => json!(pr.draft),
            "closeSourceBranch" => json!(pr.close_source_branch),
            "reviewers" => {
                Value::Array(pr.reviewers.iter().map(|a| actor_value(Some(a))).collect())
            }
            "commentCount" => pr.comment_count.map(|n| json!(n)).unwrap_or(Value::Null),
            _ => Value::Null,
        };
        obj.insert(field.clone(), v);
    }
    Value::Object(obj)
}

fn actor_value(actor: Option<&Actor>) -> Value {
    match actor {
        Some(a) => {
            let mut m = serde_json::Map::new();
            if let Some(u) = &a.username {
                m.insert("username".into(), json!(u));
            }
            if let Some(d) = &a.display_name {
                m.insert("display_name".into(), json!(d));
            }
            if let Some(uuid) = &a.uuid {
                m.insert("uuid".into(), json!(uuid));
            }
            Value::Object(m)
        }
        None => Value::Null,
    }
}

pub fn source_branch(pr: &PullRequest) -> Option<String> {
    pr.source
        .as_ref()
        .and_then(|e| e.branch.as_ref())
        .map(|b| b.name.clone())
}

pub fn destination_branch(pr: &PullRequest) -> Option<String> {
    pr.destination
        .as_ref()
        .and_then(|e| e.branch.as_ref())
        .map(|b| b.name.clone())
}

/// Pull the public html URL from the Bitbucket `links` blob.
pub fn pr_html_url(pr: &PullRequest) -> Option<String> {
    pr.links
        .as_ref()
        .and_then(|l| l.get("html"))
        .and_then(|h| h.get("href"))
        .and_then(|h| h.as_str())
        .map(str::to_string)
}

/// Pull the username from an Actor; falls back to display_name → "(unknown)".
pub fn actor_display(actor: Option<&Actor>) -> String {
    match actor {
        Some(a) => a
            .username
            .clone()
            .or_else(|| a.display_name.clone())
            .or_else(|| a.nickname.clone())
            .unwrap_or_else(|| "(unknown)".to_string()),
        None => "(unknown)".to_string(),
    }
}

/// Colorize a PR state string for human output. Unknown states return the raw input.
pub fn state_colored(state: &str, cs: &ColorScheme) -> String {
    match normalize_state(state) {
        "OPEN" => cs.green("Open"),
        "MERGED" => cs.magenta("Merged"),
        "DECLINED" => cs.red("Declined"),
        "SUPERSEDED" => cs.gray("Superseded"),
        _ => state.to_string(),
    }
}

/// Single-char status indicator for table rows: ✓ merged, ✗ declined, ○ open, etc.
pub fn state_icon(state: &str, cs: &ColorScheme) -> String {
    match normalize_state(state) {
        "OPEN" => cs.green("●"),
        "MERGED" => cs.magenta("✓"),
        "DECLINED" => cs.red("✗"),
        "SUPERSEDED" => cs.gray("○"),
        _ => "-".into(),
    }
}

fn normalize_state(state: &str) -> &str {
    let upper = state.trim().to_ascii_uppercase();
    match upper.as_str() {
        "OPEN" => "OPEN",
        "MERGED" => "MERGED",
        "DECLINED" => "DECLINED",
        "SUPERSEDED" => "SUPERSEDED",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{BranchRef, PrEndpoint};

    fn sample() -> PullRequest {
        PullRequest {
            id: 42,
            title: "Fix login".into(),
            description: Some("body".into()),
            state: Some("OPEN".into()),
            author: Some(Actor {
                username: Some("alice".into()),
                display_name: Some("Alice".into()),
                ..Default::default()
            }),
            source: Some(PrEndpoint {
                branch: Some(BranchRef {
                    name: "feature".into(),
                }),
                ..Default::default()
            }),
            destination: Some(PrEndpoint {
                branch: Some(BranchRef {
                    name: "main".into(),
                }),
                ..Default::default()
            }),
            created_on: Some("2026-05-01T00:00:00Z".into()),
            updated_on: Some("2026-05-02T00:00:00Z".into()),
            close_source_branch: true,
            draft: false,
            comment_count: Some(3),
            task_count: None,
            reviewers: vec![],
            participants: vec![],
            links: Some(json!({"html": {"href": "https://bitbucket.org/x/y/pull-requests/42"}})),
        }
    }

    #[test]
    fn projects_requested_fields() {
        let v = project_pr(
            &sample(),
            &[
                "id".into(),
                "title".into(),
                "sourceBranch".into(),
                "destinationBranch".into(),
                "url".into(),
                "draft".into(),
                "commentCount".into(),
            ],
        );
        assert_eq!(v["id"], 42);
        assert_eq!(v["title"], "Fix login");
        assert_eq!(v["sourceBranch"], "feature");
        assert_eq!(v["destinationBranch"], "main");
        assert_eq!(v["url"], "https://bitbucket.org/x/y/pull-requests/42");
        assert_eq!(v["draft"], false);
        assert_eq!(v["commentCount"], 3);
    }

    #[test]
    fn unknown_field_renders_null() {
        let v = project_pr(&sample(), &["bogus".into()]);
        assert_eq!(v["bogus"], Value::Null);
    }

    #[test]
    fn state_colored_strips_in_no_color_mode() {
        let cs = ColorScheme::new(false);
        assert_eq!(state_colored("OPEN", &cs), "Open");
        assert_eq!(state_colored("merged", &cs), "Merged");
    }
}
