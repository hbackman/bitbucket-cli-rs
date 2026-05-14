//! `/2.0/repositories/{ws}/{repo}/refs/branches` — branch listing + lookup.

use std::sync::Arc;

use url::Url;

use super::errors::ApiError;
use super::pagination::Paginated;
use super::transport::Transport;
use super::types::Branch;
use crate::bbrepo::BbRepo;

pub struct BranchService {
    pub(crate) transport: Arc<Transport>,
    pub(crate) base: Url,
}

impl BranchService {
    pub fn list(&self, repo: &BbRepo) -> Paginated<Branch> {
        let url = self
            .base
            .join(&format!(
                "repositories/{}/{}/refs/branches",
                repo.workspace, repo.slug
            ))
            .expect("static path");
        Paginated::new(self.transport.clone(), url.to_string())
    }

    pub async fn get(&self, repo: &BbRepo, name: &str) -> Result<Branch, ApiError> {
        let url = self
            .base
            .join(&format!(
                "repositories/{}/{}/refs/branches/{}",
                repo.workspace,
                repo.slug,
                urlencoding(name)
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
        self.transport.send_json(req).await
    }
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(ch),
            _ => {
                for b in ch.to_string().bytes() {
                    out.push_str(&format!("%{b:02X}"));
                }
            }
        }
    }
    out
}
