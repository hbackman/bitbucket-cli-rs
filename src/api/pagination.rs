//! Bitbucket page envelope walker.
//!
//! Bitbucket page shape: `{ "values": [...], "next": "https://...", ... }`.
//! [`Paginated`] holds the URL of the *next* page to fetch + an in-memory buffer
//! of items from the most recently fetched page. `into_stream()` produces an
//! async stream of decoded `T` values; `collect(limit)` materializes up to
//! `limit` items.

use std::collections::VecDeque;
use std::sync::Arc;

use futures::stream::Stream;
use serde::Deserialize;

use super::errors::ApiError;
use super::transport::Transport;

const DEFAULT_PAGELEN: u32 = 50;

#[derive(Debug, Deserialize)]
struct Page<T> {
    #[serde(default = "Vec::new")]
    values: Vec<T>,
    #[serde(default)]
    next: Option<String>,
}

/// Lazy paginator. Wraps an URL + transport; pages are fetched on demand.
pub struct Paginated<T> {
    transport: Arc<Transport>,
    next_url: Option<String>,
    buffer: VecDeque<T>,
    /// Default `pagelen` query param applied to the first URL when caller didn't
    /// already encode one.
    pagelen: u32,
}

impl<T> Paginated<T>
where
    T: serde::de::DeserializeOwned + Send + Unpin + 'static,
{
    pub fn new(transport: Arc<Transport>, first_url: impl Into<String>) -> Self {
        Self {
            transport,
            next_url: Some(first_url.into()),
            buffer: VecDeque::new(),
            pagelen: DEFAULT_PAGELEN,
        }
    }

    pub fn with_pagelen(mut self, pagelen: u32) -> Self {
        self.pagelen = pagelen;
        self
    }

    /// Fetch the next item, advancing pages as needed. Returns `Ok(None)` when
    /// the last page's `next` is null and the buffer is drained.
    pub async fn next_item(&mut self) -> Result<Option<T>, ApiError> {
        loop {
            if let Some(item) = self.buffer.pop_front() {
                return Ok(Some(item));
            }
            let Some(url) = self.next_url.take() else {
                return Ok(None);
            };
            let url = ensure_pagelen(&url, self.pagelen);
            let req = self
                .transport
                .http
                .get(&url)
                .build()
                .map_err(ApiError::Network)?;
            let page: Page<T> = self.transport.send_json(req).await?;
            self.buffer.extend(page.values);
            self.next_url = page.next.filter(|s| !s.is_empty());
        }
    }

    /// Collect up to `limit` items. `limit == 0` collects everything.
    pub async fn collect(mut self, limit: usize) -> Result<Vec<T>, ApiError> {
        let mut out = Vec::new();
        while let Some(item) = self.next_item().await? {
            out.push(item);
            if limit > 0 && out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    /// Turn this paginator into an async `Stream`.
    pub fn into_stream(self) -> impl Stream<Item = Result<T, ApiError>> + Send {
        let mut p = self;
        async_stream::try_stream! {
            while let Some(item) = p.next_item().await? {
                yield item;
            }
        }
    }
}

/// If the URL lacks a `pagelen` query param, append one. Cheap heuristic — for
/// already-paginated next URLs (which include `pagelen`) we leave it alone.
fn ensure_pagelen(url: &str, pagelen: u32) -> String {
    if url.contains("pagelen=") {
        return url.to_string();
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}pagelen={pagelen}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn transport_for(_server: &MockServer) -> (Arc<Transport>, tempfile::TempDir) {
        let (auth, dir) = super::super::transport::tests::auth_with_seeded_token("tok").await;
        let http = reqwest::Client::new();
        let t = Arc::new(Transport::new(http, auth, "bitbucket.org", "bb-test/0.0"));
        (t, dir)
    }

    #[tokio::test]
    async fn paginates_until_next_is_null() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/page/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "values": [1, 2],
                "next": format!("{}/page/2", server.uri())
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/page/2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "values": [3, 4]
            })))
            .mount(&server)
            .await;

        let (t, _dir) = transport_for(&server).await;
        let url = format!("{}/page/1", server.uri());
        let items: Vec<i32> = Paginated::<i32>::new(t, url).collect(0).await.unwrap();
        assert_eq!(items, vec![1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn collect_respects_limit() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/lots"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "values": [1, 2, 3, 4, 5],
                "next": format!("{}/should-not-be-called", server.uri())
            })))
            .mount(&server)
            .await;
        let (t, _dir) = transport_for(&server).await;
        let url = format!("{}/lots", server.uri());
        let items: Vec<i32> = Paginated::<i32>::new(t, url).collect(3).await.unwrap();
        assert_eq!(items, vec![1, 2, 3]);
    }
}
