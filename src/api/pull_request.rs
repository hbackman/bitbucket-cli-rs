//! `/2.0/repositories/{ws}/{repo}/pullrequests` — full PR surface.

use std::sync::Arc;

use url::Url;

use super::errors::ApiError;
use super::pagination::Paginated;
use super::transport::Transport;
use super::types::{
    Activity, Actor, BuildStatus, Comment, Commit, CreatePr, MergeInput, PrState, PullRequest,
    UpdatePr,
};
use crate::bbrepo::BbRepo;

pub struct PullRequestService {
    pub(crate) transport: Arc<Transport>,
    pub(crate) base: Url,
}

/// Optional filters for the list endpoint.
#[derive(Debug, Default, Clone)]
pub struct ListOpts {
    pub state: Option<PrState>,
    /// Bitbucket query DSL — passed through as `?q=`.
    pub query: Option<String>,
    pub author: Option<String>,
    pub page_len: Option<u32>,
    pub fields: Option<String>,
}

impl PullRequestService {
    pub fn list(&self, repo: &BbRepo, opts: ListOpts) -> Paginated<PullRequest> {
        let mut url = self.prs_url(repo);
        {
            let mut q = url.query_pairs_mut();
            if let Some(s) = opts.state {
                q.append_pair("state", s.as_str());
            }
            if let Some(v) = opts.query {
                q.append_pair("q", &v);
            }
            if let Some(v) = opts.author {
                // Bitbucket accepts `author.username` queries embedded in the q param.
                q.append_pair("q", &format!("author.username=\"{v}\""));
            }
            if let Some(n) = opts.page_len {
                q.append_pair("pagelen", &n.to_string());
            }
            if let Some(v) = opts.fields {
                q.append_pair("fields", &v);
            }
        }
        Paginated::new(self.transport.clone(), url.to_string())
    }

    pub async fn get(&self, repo: &BbRepo, id: u32) -> Result<PullRequest, ApiError> {
        let req = self
            .transport
            .http
            .get(self.pr_url(repo, id))
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_json(req).await
    }

    pub async fn create(&self, repo: &BbRepo, input: &CreatePr) -> Result<PullRequest, ApiError> {
        let req = self
            .transport
            .http
            .post(self.prs_url(repo))
            .json(input)
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_json(req).await
    }

    pub async fn update(
        &self,
        repo: &BbRepo,
        id: u32,
        input: &UpdatePr,
    ) -> Result<PullRequest, ApiError> {
        let req = self
            .transport
            .http
            .put(self.pr_url(repo, id))
            .json(input)
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_json(req).await
    }

    pub async fn merge(
        &self,
        repo: &BbRepo,
        id: u32,
        input: &MergeInput,
    ) -> Result<PullRequest, ApiError> {
        let req = self
            .transport
            .http
            .post(self.subpath(repo, id, "merge"))
            .json(input)
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_json(req).await
    }

    pub async fn decline(&self, repo: &BbRepo, id: u32) -> Result<PullRequest, ApiError> {
        let req = self
            .transport
            .http
            .post(self.subpath(repo, id, "decline"))
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_json(req).await
    }

    pub async fn approve(&self, repo: &BbRepo, id: u32) -> Result<(), ApiError> {
        let req = self
            .transport
            .http
            .post(self.subpath(repo, id, "approve"))
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_void(req).await
    }

    pub async fn unapprove(&self, repo: &BbRepo, id: u32) -> Result<(), ApiError> {
        let req = self
            .transport
            .http
            .delete(self.subpath(repo, id, "approve"))
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_void(req).await
    }

    pub async fn request_changes(&self, repo: &BbRepo, id: u32) -> Result<(), ApiError> {
        let req = self
            .transport
            .http
            .post(self.subpath(repo, id, "request-changes"))
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_void(req).await
    }

    pub async fn unrequest_changes(&self, repo: &BbRepo, id: u32) -> Result<(), ApiError> {
        let req = self
            .transport
            .http
            .delete(self.subpath(repo, id, "request-changes"))
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_void(req).await
    }

    /// Diff is potentially huge; return the raw response so the caller can stream
    /// it to stdout without buffering 100MB into memory.
    pub async fn diff(&self, repo: &BbRepo, id: u32) -> Result<reqwest::Response, ApiError> {
        let req = self
            .transport
            .http
            .get(self.subpath(repo, id, "diff"))
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_stream(req).await
    }

    pub fn commits(&self, repo: &BbRepo, id: u32) -> Paginated<Commit> {
        Paginated::new(self.transport.clone(), self.subpath(repo, id, "commits"))
    }

    pub fn comments(&self, repo: &BbRepo, id: u32) -> Paginated<Comment> {
        Paginated::new(self.transport.clone(), self.subpath(repo, id, "comments"))
    }

    pub async fn add_comment(
        &self,
        repo: &BbRepo,
        id: u32,
        body: &str,
    ) -> Result<Comment, ApiError> {
        let payload = serde_json::json!({
            "content": { "raw": body }
        });
        let req = self
            .transport
            .http
            .post(self.subpath(repo, id, "comments"))
            .json(&payload)
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_json(req).await
    }

    pub async fn edit_comment(
        &self,
        repo: &BbRepo,
        id: u32,
        comment_id: u64,
        body: &str,
    ) -> Result<Comment, ApiError> {
        let payload = serde_json::json!({
            "content": { "raw": body }
        });
        let req = self
            .transport
            .http
            .put(self.subpath(repo, id, &format!("comments/{comment_id}")))
            .json(&payload)
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_json(req).await
    }

    pub async fn delete_comment(
        &self,
        repo: &BbRepo,
        id: u32,
        comment_id: u64,
    ) -> Result<(), ApiError> {
        let req = self
            .transport
            .http
            .delete(self.subpath(repo, id, &format!("comments/{comment_id}")))
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_void(req).await
    }

    /// `GET /2.0/repositories/{ws}/{repo}/effective-default-reviewers` — list of
    /// users automatically added as reviewers to new PRs in this repo.
    pub fn effective_default_reviewers(&self, repo: &BbRepo) -> Paginated<Actor> {
        let url = self
            .base
            .join(&format!(
                "repositories/{}/{}/effective-default-reviewers",
                repo.workspace, repo.slug
            ))
            .expect("static path is well-formed");
        Paginated::new(self.transport.clone(), url.to_string())
    }

    pub fn statuses(&self, repo: &BbRepo, id: u32) -> Paginated<BuildStatus> {
        Paginated::new(self.transport.clone(), self.subpath(repo, id, "statuses"))
    }

    pub fn activity(&self, repo: &BbRepo, id: u32) -> Paginated<Activity> {
        Paginated::new(self.transport.clone(), self.subpath(repo, id, "activity"))
    }

    // --- url builders ----------------------------------------------------

    fn prs_url(&self, repo: &BbRepo) -> Url {
        self.base
            .join(&format!(
                "repositories/{}/{}/pullrequests",
                repo.workspace, repo.slug
            ))
            .expect("static path is well-formed")
    }

    fn pr_url(&self, repo: &BbRepo, id: u32) -> Url {
        self.base
            .join(&format!(
                "repositories/{}/{}/pullrequests/{id}",
                repo.workspace, repo.slug
            ))
            .expect("static path is well-formed")
    }

    fn subpath(&self, repo: &BbRepo, id: u32, sub: &str) -> Url {
        self.base
            .join(&format!(
                "repositories/{}/{}/pullrequests/{id}/{sub}",
                repo.workspace, repo.slug
            ))
            .expect("static path is well-formed")
    }
}
