//! Bitbucket REST API client.
//!
//! Spec: `docs/specs/04-api-client.md`.
//!
//! Three layers:
//! 1. [`Transport`] — auth header injection, 429 / 401-refresh retries, error
//!    envelope parsing, debug logging.
//! 2. [`Client`] — typed entry point exposing resource services.
//! 3. [`crate::cli::api`] — the `bbk api` escape hatch built on top.

pub mod activity;
pub mod branch;
pub mod cache;
pub mod client;
pub mod comment;
pub mod debug;
pub mod errors;
pub mod pagination;
pub mod pull_request;
pub mod repository;
pub mod status;
pub mod transport;
pub mod types;
pub mod user;
pub mod workspace;

pub use client::{Client, DEFAULT_BASE};
pub use debug::DebugMode;
pub use errors::{ApiError, BitbucketError};
pub use pagination::Paginated;
pub use transport::{ApiResponse, Transport};
pub use types::{
    Activity, Actor, Branch, BranchInput, BranchRef, BuildStatus, Comment, CommentRef, Commit,
    CommitAuthor, CommitRef, CreatePr, InlineLocation, MainBranch, MergeInput, Participant,
    PrEndpoint, PrEndpointInput, PrState, PullRequest, RenderedContent, RepoRef, Repository,
    ReviewerInput, UpdatePr, User, Workspace,
};

/// Build a Bitbucket API client from a context's HTTP + AuthSource.
///
/// Used by `Context::api()` to lazily wire the client on first use, and by tests
/// that point the base URL at a `wiremock` server.
pub fn build_client(
    http: reqwest::Client,
    auth: std::sync::Arc<crate::auth::AuthSource>,
    host: &str,
    user_agent: &str,
) -> Client {
    let transport = std::sync::Arc::new(Transport::new(http, auth, host, user_agent));
    Client::new(transport)
}
