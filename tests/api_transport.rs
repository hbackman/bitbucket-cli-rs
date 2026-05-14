//! Integration tests for `crate::api::Transport` and the typed `Client`,
//! covering the spec-04 acceptance criteria. We point the API base URL at a
//! local `wiremock` server. To avoid the global-env-var trap, each test
//! pre-seeds an `api_token` credential in an in-memory `Hosts` rather than
//! flipping `BB_TOKEN`.

use std::sync::Arc;

use bb::api::{self, ApiError, Client, Transport};
use bb::auth::{AuthSource, KeyringBackend, MemKeyring};
use bb::bbrepo::BbRepo;
use bb::config::Hosts;
use serde_json::json;
use serde_yaml::{Mapping, Value};
use tempfile::TempDir;
use tokio::sync::RwLock;
use wiremock::matchers::{header, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// Pre-loaded auth wrapping an `api_token` for `test-user@bitbucket.org`. The
/// token survives in memory via plaintext (`oauth_token`) so we don't touch
/// the real OS keyring.
async fn auth_with_token(token: &str) -> (Arc<AuthSource>, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hosts.yml");
    let mut hosts = Hosts::load_from(&path).await.unwrap();

    let mut block = Mapping::new();
    block.insert(Value::String("type".into()), Value::String("api_token".into()));
    block.insert(
        Value::String("oauth_token".into()),
        Value::String(token.into()),
    );
    block.insert(
        Value::String("git_protocol".into()),
        Value::String("https".into()),
    );
    hosts
        .set_user_block("bitbucket.org", "test-user", block)
        .await
        .unwrap();
    hosts
        .set("bitbucket.org", "active_user", "test-user")
        .await
        .unwrap();

    let hosts = Arc::new(RwLock::new(hosts));
    let kr: Arc<dyn KeyringBackend> = Arc::new(MemKeyring::new());
    let http = reqwest::Client::new();
    (Arc::new(AuthSource::new(hosts, kr, http)), dir)
}

async fn client_for(server: &MockServer, token: &str) -> (Client, TempDir) {
    let (auth, dir) = auth_with_token(token).await;
    let http = reqwest::Client::new();
    let transport = Arc::new(Transport::new(http, auth, "bitbucket.org", "bb-test/0.0"));
    let base = url::Url::parse(&format!("{}/", server.uri())).unwrap();
    (Client::with_base(transport, base), dir)
}

#[tokio::test]
async fn injects_authorization_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", "Bearer the-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"username": "hbackman"})))
        .mount(&server)
        .await;
    let (client, _dir) = client_for(&server, "the-token").await;
    let user = client.user().current().await.unwrap();
    assert_eq!(user.username, "hbackman");
}

#[tokio::test]
async fn parses_bitbucket_error_envelope() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repositories/x/y"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "type": "error",
            "error": { "message": "Repository not found." }
        })))
        .mount(&server)
        .await;
    let (client, _dir) = client_for(&server, "tok").await;
    let repo = BbRepo::new("x", "y");
    let err = client.repositories().get(&repo).await.unwrap_err();
    match err {
        ApiError::Response(r) => {
            assert_eq!(r.status.as_u16(), 404);
            assert!(r.message.contains("Repository not found"));
        }
        other => panic!("expected Response, got {other:?}"),
    }
}

#[tokio::test]
async fn rate_limit_retries_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "0")
                .set_body_string(""),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"username": "x"})))
        .mount(&server)
        .await;
    let (client, _dir) = client_for(&server, "tok").await;
    let user = client.user().current().await.unwrap();
    assert_eq!(user.username, "x");
}

#[tokio::test]
async fn rate_limit_exhausts_budget_after_two_retries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "0")
                .set_body_string(""),
        )
        .mount(&server)
        .await;
    let (client, _dir) = client_for(&server, "tok").await;
    let err = client.user().current().await.unwrap_err();
    assert!(matches!(err, ApiError::RateLimit { .. }));
}

#[tokio::test]
async fn unauthorized_for_api_token_returns_auth_error() {
    let server = MockServer::start().await;
    // api_token credentials don't support refresh — a 401 should immediately
    // bubble out as an Auth error.
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(401).set_body_string(""))
        .mount(&server)
        .await;
    let (client, _dir) = client_for(&server, "tok").await;
    let err = client.user().current().await.unwrap_err();
    assert!(matches!(err, ApiError::Auth { .. }));
}

#[tokio::test]
async fn paginates_follows_next() {
    let server = MockServer::start().await;
    // Use a distinct path for the second page so wiremock can't ambiguously
    // match the first-page mock against the second-page request.
    let next_url = format!("{}/page-two", server.uri());
    Mock::given(method("GET"))
        .and(path("/repositories/ws/repo/pullrequests"))
        .and(query_param("pagelen", "50"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "values": [{"id": 1, "title": "first"}],
                "next": next_url
            })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/page-two"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "values": [{"id": 2, "title": "second"}, {"id": 3, "title": "third"}]
        })))
        .mount(&server)
        .await;
    let (client, _dir) = client_for(&server, "tok").await;
    let repo = BbRepo::new("ws", "repo");
    let all = client
        .pull_requests()
        .list(&repo, Default::default())
        .collect(0)
        .await
        .unwrap();
    let ids: Vec<u32> = all.iter().map(|p| p.id).collect();
    assert_eq!(ids, vec![1, 2, 3]);
}

#[tokio::test]
async fn create_pr_sends_json_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/repositories/ws/repo/pullrequests"))
        .and(header_exists("content-type"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 42,
            "title": "Hello",
            "state": "OPEN"
        })))
        .mount(&server)
        .await;
    let (client, _dir) = client_for(&server, "tok").await;
    let repo = BbRepo::new("ws", "repo");
    let input = api::CreatePr {
        title: "Hello".into(),
        source: api::PrEndpointInput {
            branch: api::BranchInput {
                name: "feature".into(),
            },
        },
        ..Default::default()
    };
    let pr = client.pull_requests().create(&repo, &input).await.unwrap();
    assert_eq!(pr.id, 42);
    assert_eq!(pr.title, "Hello");

    let received: Vec<Request> = server.received_requests().await.unwrap();
    let post = received
        .iter()
        .find(|r| r.url.path() == "/repositories/ws/repo/pullrequests")
        .expect("POST recorded");
    let body: serde_json::Value = serde_json::from_slice(&post.body).unwrap();
    assert_eq!(body["title"], "Hello");
    assert_eq!(body["source"]["branch"]["name"], "feature");
}
