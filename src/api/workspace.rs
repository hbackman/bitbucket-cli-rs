//! `/2.0/workspaces` — workspaces accessible by the current user.

use std::sync::Arc;

use super::errors::ApiError;
use super::pagination::Paginated;
use super::transport::Transport;
use super::types::Workspace;

pub struct WorkspaceService {
    pub(crate) transport: Arc<Transport>,
    pub(crate) base: url::Url,
}

impl WorkspaceService {
    /// `GET /2.0/workspaces` — paginated.
    pub fn list(&self) -> Paginated<Workspace> {
        let url = self
            .base
            .join("workspaces")
            .expect("workspaces is a literal segment");
        Paginated::new(self.transport.clone(), url.to_string())
    }

    /// `GET /2.0/workspaces/{slug}` — fetch one workspace by slug.
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
