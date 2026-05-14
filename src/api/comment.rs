//! PR / commit comments. For MVP the only writer is PR comments — see
//! [`super::pull_request::PullRequestService::add_comment`]. This module exists
//! to host commit-comments and inline-comment helpers when they land.

pub use super::types::{Comment, CommentRef, InlineLocation, RenderedContent};
