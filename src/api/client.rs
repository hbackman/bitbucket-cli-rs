//! Top-level typed client. Holds a shared [`Transport`] and hands out service
//! views into each resource.

use std::sync::Arc;

use url::Url;

use super::branch::BranchService;
use super::pull_request::PullRequestService;
use super::repository::RepositoryService;
use super::transport::Transport;
use super::user::UserService;
use super::workspace::WorkspaceService;

pub const DEFAULT_BASE: &str = "https://api.bitbucket.org/2.0/";

#[derive(Clone)]
pub struct Client {
    transport: Arc<Transport>,
    base: Url,
}

impl Client {
    pub fn new(transport: Arc<Transport>) -> Self {
        Self {
            transport,
            base: Url::parse(DEFAULT_BASE).expect("default base URL is valid"),
        }
    }

    /// Override the API base URL. Used in tests to point at `wiremock`.
    pub fn with_base(transport: Arc<Transport>, base: Url) -> Self {
        Self { transport, base }
    }

    pub fn transport(&self) -> Arc<Transport> {
        self.transport.clone()
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    pub fn user(&self) -> UserService {
        UserService {
            transport: self.transport.clone(),
            base: self.base.clone(),
        }
    }

    pub fn workspaces(&self) -> WorkspaceService {
        WorkspaceService {
            transport: self.transport.clone(),
            base: self.base.clone(),
        }
    }

    pub fn repositories(&self) -> RepositoryService {
        RepositoryService {
            transport: self.transport.clone(),
            base: self.base.clone(),
        }
    }

    pub fn branches(&self) -> BranchService {
        BranchService {
            transport: self.transport.clone(),
            base: self.base.clone(),
        }
    }

    pub fn pull_requests(&self) -> PullRequestService {
        PullRequestService {
            transport: self.transport.clone(),
            base: self.base.clone(),
        }
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").field("base", &self.base).finish()
    }
}
