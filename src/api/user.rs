//! `/2.0/user` — current user.

use std::sync::Arc;

use super::errors::ApiError;
use super::transport::Transport;
use super::types::User;

pub struct UserService {
    pub(crate) transport: Arc<Transport>,
    pub(crate) base: url::Url,
}

impl UserService {
    /// `GET /2.0/user` — the authenticated user.
    pub async fn current(&self) -> Result<User, ApiError> {
        let url = self.base.join("user").map_err(invalid_url)?;
        let req = self
            .transport
            .http
            .get(url)
            .build()
            .map_err(ApiError::Network)?;
        self.transport.send_json(req).await
    }
}

fn invalid_url(e: url::ParseError) -> ApiError {
    ApiError::Auth {
        hint: format!("invalid URL: {e}"),
    }
}
