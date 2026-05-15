//! `/2.0/repositories/...` — repository CRUD and listing.

use std::sync::Arc;

use serde::Serialize;
use url::Url;

use super::errors::ApiError;
use super::pagination::Paginated;
use super::transport::Transport;
use super::types::Repository;
use crate::bbrepo::BbRepo;

pub struct RepositoryService {
    pub(crate) transport: Arc<Transport>,
    pub(crate) base: Url,
}

/// Optional filters for the list endpoint.
#[derive(Debug, Default, Clone)]
pub struct ListOpts {
    /// Bitbucket query DSL — passed through as `?q=`.
    pub query: Option<String>,
    /// `role` filter: `owner`, `admin`, `contributor`, `member`.
    pub role: Option<String>,
    /// Page size hint.
    pub pagelen: Option<u32>,
    /// Sparse-fieldset selector (`?fields=`).
    pub fields: Option<String>,
    /// `?sort=` field, e.g. `-updated_on`.
    pub sort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CreateRepo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub is_private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mainbranch: Option<MainBranchInput>,
    #[serde(rename = "scm")]
    pub scm: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectRef {
    pub key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MainBranchInput {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
}

/// `POST /repositories/{ws}/{repo}/forks` body.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ForkInput {
    /// Override the fork repo name. Defaults to the parent's slug.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Target workspace for the fork. Defaults to the current user's workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceRef>,
    /// When true, only the default branch is forked. Bitbucket-specific flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceRef {
    pub slug: String,
}

impl CreateRepo {
    pub fn new() -> Self {
        Self {
            scm: "git",
            ..Default::default()
        }
    }
}

impl RepositoryService {
    /// `GET /2.0/repositories/{workspace}` — list repos in a workspace.
    pub fn list(&self, workspace: &str, opts: ListOpts) -> Paginated<Repository> {
        let mut url = self
            .base
            .join(&format!("repositories/{workspace}"))
            .expect("static path");
        {
            let mut q = url.query_pairs_mut();
            if let Some(v) = opts.query.as_deref() {
                q.append_pair("q", v);
            }
            if let Some(v) = opts.role.as_deref() {
                q.append_pair("role", v);
            }
            if let Some(n) = opts.pagelen {
                q.append_pair("pagelen", &n.to_string());
            }
            if let Some(v) = opts.fields.as_deref() {
                q.append_pair("fields", v);
            }
            if let Some(v) = opts.sort.as_deref() {
                q.append_pair("sort", v);
            }
        }
        Paginated::new(self.transport.clone(), url.to_string())
    }

    /// `GET /2.0/repositories/{workspace}/{slug}` — one repo.
    pub async fn get(&self, repo: &BbRepo) -> Result<Repository, ApiError> {
        let url = self.repo_url(repo)?;
        let req = self
            .transport
            .http
            .get(url)
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_json(req).await
    }

    /// `POST /2.0/repositories/{workspace}/{slug}` — create a repo.
    pub async fn create(&self, repo: &BbRepo, input: &CreateRepo) -> Result<Repository, ApiError> {
        let url = self.repo_url(repo)?;
        let req = self
            .transport
            .http
            .post(url)
            .json(input)
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_json(req).await
    }

    /// `POST /2.0/repositories/{workspace}/{slug}/forks` — fork a repo.
    pub async fn fork(&self, repo: &BbRepo, input: &ForkInput) -> Result<Repository, ApiError> {
        let url = self
            .base
            .join(&format!(
                "repositories/{}/{}/forks",
                repo.workspace, repo.slug
            ))
            .map_err(|e| ApiError::Auth {
                hint: format!("invalid URL: {e}"),
            })?;
        let req = self
            .transport
            .http
            .post(url)
            .json(input)
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_json(req).await
    }

    /// `GET /2.0/repositories/{workspace}/{slug}/src/{ref}/{path}` — raw file contents.
    pub async fn read_source(
        &self,
        repo: &BbRepo,
        rev: &str,
        path: &str,
    ) -> Result<bytes::Bytes, ApiError> {
        let url = self
            .base
            .join(&format!(
                "repositories/{}/{}/src/{rev}/{path}",
                repo.workspace, repo.slug,
            ))
            .map_err(|e| ApiError::Auth {
                hint: format!("invalid URL: {e}"),
            })?;
        let req = self
            .transport
            .http
            .get(url)
            .build()
            .map_err(ApiError::Network)?;
        let resp = self.transport.send(req).await?;
        Ok(resp.body)
    }

    /// `DELETE /2.0/repositories/{workspace}/{slug}`.
    pub async fn delete(&self, repo: &BbRepo) -> Result<(), ApiError> {
        let url = self.repo_url(repo)?;
        let req = self
            .transport
            .http
            .delete(url)
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_void(req).await
    }

    fn repo_url(&self, repo: &BbRepo) -> Result<Url, ApiError> {
        self.base
            .join(&format!("repositories/{}/{}", repo.workspace, repo.slug))
            .map_err(|e| ApiError::Auth {
                hint: format!("invalid URL: {e}"),
            })
    }
}
