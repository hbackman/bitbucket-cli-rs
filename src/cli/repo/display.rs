//! Shared rendering helpers for `bbk repo …` JSON output.
//!
//! Every list/view subcommand exposes the same `--json` field surface, so the
//! field set and the projection live here.

use serde_json::{json, Value};

use crate::api::types::Repository;

/// `--json` fields supported by `bbk repo view` and `bbk repo list`.
pub const JSON_FIELDS: &[&str] = &[
    "name",
    "fullName",
    "owner",
    "description",
    "isPrivate",
    "mainBranch",
    "language",
    "createdOn",
    "updatedOn",
    "url",
    "size",
];

/// Project a [`Repository`] into a JSON map keyed by the user-facing field names.
pub fn project_repo(repo: &Repository, fields: &[String]) -> Value {
    let mut obj = serde_json::Map::new();
    for field in fields {
        let v = match field.as_str() {
            "name" => json!(repo.name),
            "fullName" => json!(repo.full_name),
            "owner" => owner_value(repo),
            "description" => match &repo.description {
                Some(s) => json!(s),
                None => Value::Null,
            },
            "isPrivate" => json!(repo.is_private),
            "mainBranch" => match repo.mainbranch.as_ref().map(|b| b.name.as_str()) {
                Some(s) => json!(s),
                None => Value::Null,
            },
            "language" => match &repo.language {
                Some(s) => json!(s),
                None => Value::Null,
            },
            "createdOn" => match &repo.created_on {
                Some(s) => json!(s),
                None => Value::Null,
            },
            "updatedOn" => match &repo.updated_on {
                Some(s) => json!(s),
                None => Value::Null,
            },
            "url" => match repo_html_url(repo) {
                Some(s) => json!(s),
                None => Value::Null,
            },
            "size" => match repo.size {
                Some(n) => json!(n),
                None => Value::Null,
            },
            _ => Value::Null,
        };
        obj.insert(field.clone(), v);
    }
    Value::Object(obj)
}

fn owner_value(repo: &Repository) -> Value {
    match &repo.owner {
        Some(actor) => {
            let mut m = serde_json::Map::new();
            if let Some(u) = &actor.username {
                m.insert("username".into(), json!(u));
            }
            if let Some(d) = &actor.display_name {
                m.insert("display_name".into(), json!(d));
            }
            if let Some(uuid) = &actor.uuid {
                m.insert("uuid".into(), json!(uuid));
            }
            Value::Object(m)
        }
        None => Value::Null,
    }
}

/// Extract the public html URL from Bitbucket's `links` blob if present.
pub fn repo_html_url(repo: &Repository) -> Option<String> {
    repo.links
        .as_ref()
        .and_then(|l| l.get("html"))
        .and_then(|h| h.get("href"))
        .and_then(|h| h.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{Actor, MainBranch};

    fn sample() -> Repository {
        Repository {
            full_name: "acme/widgets".into(),
            name: "widgets".into(),
            uuid: Some("{uuid}".into()),
            description: Some("a widget shop".into()),
            is_private: true,
            language: Some("rust".into()),
            mainbranch: Some(MainBranch {
                name: "main".into(),
                kind: Some("branch".into()),
            }),
            created_on: Some("2024-01-01T00:00:00Z".into()),
            updated_on: Some("2024-06-01T00:00:00Z".into()),
            size: Some(12345),
            owner: Some(Actor {
                uuid: Some("{owner-uuid}".into()),
                username: Some("acme".into()),
                display_name: Some("Acme".into()),
                nickname: None,
                kind: Some("team".into()),
            }),
            links: Some(serde_json::json!({
                "html": { "href": "https://bitbucket.org/acme/widgets" }
            })),
            parent: None,
        }
    }

    #[test]
    fn projects_requested_fields() {
        let v = project_repo(
            &sample(),
            &[
                "name".into(),
                "fullName".into(),
                "isPrivate".into(),
                "mainBranch".into(),
                "url".into(),
            ],
        );
        assert_eq!(v["name"], "widgets");
        assert_eq!(v["fullName"], "acme/widgets");
        assert_eq!(v["isPrivate"], true);
        assert_eq!(v["mainBranch"], "main");
        assert_eq!(v["url"], "https://bitbucket.org/acme/widgets");
    }

    #[test]
    fn unknown_field_renders_null() {
        let v = project_repo(&sample(), &["surprise".into()]);
        assert_eq!(v["surprise"], Value::Null);
    }

    #[test]
    fn missing_optional_fields_render_null() {
        let mut r = sample();
        r.description = None;
        r.language = None;
        let v = project_repo(&r, &["description".into(), "language".into()]);
        assert_eq!(v["description"], Value::Null);
        assert_eq!(v["language"], Value::Null);
    }
}
