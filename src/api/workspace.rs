//! Workspace endpoints.
//!
//! Bitbucket CHANGE-2770 retired the cross-workspace `/2.0/workspaces` and
//! `/2.0/user/permissions/workspaces` endpoints. The replacement for listing
//! workspaces the authenticated user belongs to is `GET /2.0/user/workspaces`,
//! which returns paginated `workspace_access` objects (a wrapper around the
//! underlying [`Workspace`]).

use std::sync::Arc;

use serde::Deserialize;

use super::errors::ApiError;
use super::pagination::Paginated;
use super::transport::Transport;
use super::types::Workspace;

pub struct WorkspaceService {
    pub(crate) transport: Arc<Transport>,
    pub(crate) base: url::Url,
}

/// One entry from `GET /2.0/user/workspaces`. Each describes the caller's
/// access to a single workspace; the embedded [`Workspace`] is the value most
/// callers actually want.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceMembership {
    pub workspace: Workspace,
    #[serde(default)]
    pub permission: Option<String>,
}

impl WorkspaceService {
    /// `GET /2.0/user/workspaces` — every workspace the authenticated user is
    /// a member of. Each page value is a [`WorkspaceMembership`] wrapper.
    pub fn list(&self) -> Paginated<WorkspaceMembership> {
        let url = self
            .base
            .join("user/workspaces")
            .expect("user/workspaces is a literal segment");
        Paginated::new(self.transport.clone(), url.to_string())
    }

    /// `GET /2.0/workspaces/{slug}` — fetch a single workspace by slug. Still
    /// supported per Bitbucket's deprecation note (only the cross-workspace
    /// listing was retired).
    pub async fn get(&self, slug: &str) -> Result<Workspace, ApiError> {
        let url = self
            .base
            .join(&format!("workspaces/{slug}"))
            .map_err(|e| ApiError::Auth {
                hint: format!("invalid URL: {e}"),
            })?;
        let req = self
            .transport
            .http
            .get(url)
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_json(req).await
    }
}
