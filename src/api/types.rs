//! Typed payloads shared across resource modules.
//!
//! Only the fields commands actually use are deserialized. The catch-all
//! `serde(default)` lets us survive Bitbucket's habit of adding fields without
//! warning.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Generic actor (user) — Bitbucket's `account` polymorphism collapsed into one
/// shape, since for MVP we never need to distinguish app passwords / pipelines
/// / etc. from a regular user.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Actor {
    #[serde(default)]
    pub uuid: Option<String>,
    /// User-handle on Bitbucket Cloud. Atlassian-managed accounts may omit this.
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub full_name: String,
    pub name: String,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub mainbranch: Option<MainBranch>,
    #[serde(default)]
    pub created_on: Option<String>,
    #[serde(default)]
    pub updated_on: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub owner: Option<Actor>,
    #[serde(default)]
    pub links: Option<serde_json::Value>,
    /// Present when this repo is a fork; carries the parent's `full_name`.
    #[serde(default)]
    pub parent: Option<RepoRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainBranch {
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub slug: String,
    // `GET /2.0/user/workspaces` returns workspace_access objects whose embedded
    // workspace omits `name`; only `/2.0/workspaces/{slug}` includes it. Treat
    // as optional and fall back to `slug` for display.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub is_private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub name: String,
    #[serde(default)]
    pub target: Option<CommitRef>,
}

/// Lightweight reference to a commit. Used by branch/PR endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitRef {
    pub hash: String,
    #[serde(default)]
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    pub hash: String,
    #[serde(default)]
    pub author: Option<CommitAuthor>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitAuthor {
    #[serde(default)]
    pub raw: Option<String>,
    #[serde(default)]
    pub user: Option<Actor>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum PrState {
    Open,
    Merged,
    Declined,
    Superseded,
}

impl PrState {
    pub fn as_str(self) -> &'static str {
        match self {
            PrState::Open => "OPEN",
            PrState::Merged => "MERGED",
            PrState::Declined => "DECLINED",
            PrState::Superseded => "SUPERSEDED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub id: u32,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub author: Option<Actor>,
    #[serde(default)]
    pub source: Option<PrEndpoint>,
    #[serde(default)]
    pub destination: Option<PrEndpoint>,
    #[serde(default)]
    pub created_on: Option<String>,
    #[serde(default)]
    pub updated_on: Option<String>,
    #[serde(default)]
    pub close_source_branch: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub comment_count: Option<u32>,
    #[serde(default)]
    pub task_count: Option<u32>,
    #[serde(default)]
    pub reviewers: Vec<Actor>,
    #[serde(default)]
    pub participants: Vec<Participant>,
    #[serde(default)]
    pub links: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrEndpoint {
    #[serde(default)]
    pub branch: Option<BranchRef>,
    #[serde(default)]
    pub commit: Option<CommitRef>,
    #[serde(default)]
    pub repository: Option<RepoRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchRef {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRef {
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    #[serde(default)]
    pub user: Option<Actor>,
    /// Bitbucket sends `PARTICIPANT` or `REVIEWER`.
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub approved: bool,
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: u64,
    #[serde(default)]
    pub user: Option<Actor>,
    #[serde(default)]
    pub content: Option<RenderedContent>,
    #[serde(default)]
    pub created_on: Option<String>,
    #[serde(default)]
    pub updated_on: Option<String>,
    #[serde(default)]
    pub inline: Option<InlineLocation>,
    #[serde(default)]
    pub parent: Option<CommentRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentRef {
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedContent {
    #[serde(default)]
    pub raw: Option<String>,
    #[serde(default)]
    pub markup: Option<String>,
    #[serde(default)]
    pub html: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineLocation {
    pub path: String,
    #[serde(default)]
    pub from: Option<u32>,
    #[serde(default)]
    pub to: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildStatus {
    pub key: String,
    pub state: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub created_on: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub updated_on: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    #[serde(default)]
    pub update: Option<serde_json::Value>,
    #[serde(default)]
    pub approval: Option<serde_json::Value>,
    #[serde(default)]
    pub comment: Option<Comment>,
    #[serde(default)]
    pub changes_requested: Option<serde_json::Value>,
}

/// Body for `POST /pullrequests`. Optional fields are omitted from the JSON via
/// `skip_serializing_if`. `Deserialize` is also derived so failed-create payloads
/// can round-trip through the `bbk pr create --recover` file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreatePr {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source: PrEndpointInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<PrEndpointInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_source_branch: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewers: Vec<ReviewerInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrEndpointInput {
    pub branch: BranchInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BranchInput {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReviewerInput {
    pub uuid: String,
}

/// Body for `PUT /pullrequests/{id}`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct UpdatePr {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<PrEndpointInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewers: Option<Vec<ReviewerInput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<bool>,
}

/// Body for `POST /pullrequests/{id}/merge`. Empty body merges with defaults.
#[derive(Debug, Clone, Serialize, Default)]
pub struct MergeInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_source_branch: Option<bool>,
}
