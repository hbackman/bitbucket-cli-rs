//! End-to-end `bb api` smoke tests.
//!
//! Each test spawns the compiled binary against a `wiremock` server, supplying
//! `BB_TOKEN` for auth and `BB_CONFIG_DIR` pointed at a tempdir so the run
//! doesn't touch the user's real config. The endpoint is given as a full
//! `http://127.0.0.1:PORT/path` URL — the API command passes those through
//! verbatim, bypassing the bitbucket.org prefix construction.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn bb() -> Command {
    Command::cargo_bin("bb").expect("bb binary built")
}

fn with_token_env<'a>(
    cmd: &'a mut Command,
    dir: &std::path::Path,
    token: &str,
) -> &'a mut Command {
    cmd.env("BB_CONFIG_DIR", dir)
        .env("BB_TOKEN", token)
        .env_remove("BITBUCKET_TOKEN")
}

#[tokio::test]
async fn bb_api_get_returns_json_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/2.0/user"))
        .and(header("authorization", "Bearer the-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"username": "hbackman"})))
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let url = format!("{}/2.0/user", server.uri());
    let mut cmd = bb();
    with_token_env(&mut cmd, dir.path(), "the-token")
        .args(["api", &url])
        .assert()
        .success()
        .stdout(predicate::str::contains("hbackman"));
}

#[tokio::test]
async fn bb_api_jq_filters_output() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/2.0/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"username": "hbackman"})))
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let url = format!("{}/2.0/user", server.uri());
    let mut cmd = bb();
    with_token_env(&mut cmd, dir.path(), "tok")
        .args(["api", &url, "--jq", ".username"])
        .assert()
        .success()
        .stdout("hbackman\n");
}

#[tokio::test]
async fn bb_api_field_sends_post_with_json_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/2.0/thing"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let url = format!("{}/2.0/thing", server.uri());
    let mut cmd = bb();
    with_token_env(&mut cmd, dir.path(), "tok")
        .args(["api", &url, "-F", "a=1", "-F", "b=true"])
        .assert()
        .success();

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(&requests.last().unwrap().body).unwrap();
    assert_eq!(body, json!({"a": 1, "b": true}));
}

#[tokio::test]
async fn bb_api_paginate_slurp_collects_all_values() {
    let server = MockServer::start().await;
    let next = format!("{}/2.0/page-two", server.uri());
    Mock::given(method("GET"))
        .and(path("/2.0/things"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "values": [1, 2],
            "next": next
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.0/page-two"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "values": [3, 4, 5]
        })))
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let url = format!("{}/2.0/things", server.uri());
    let mut cmd = bb();
    let assertion = with_token_env(&mut cmd, dir.path(), "tok")
        .args(["api", &url, "--paginate", "--slurp"])
        .assert()
        .success();
    let stdout = std::str::from_utf8(&assertion.get_output().stdout).unwrap();
    let arr: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(arr, json!([1, 2, 3, 4, 5]));
}

#[tokio::test]
async fn bb_api_placeholders_use_repo_override() {
    let server = MockServer::start().await;
    // Mount a wiremock route that matches the workspace/repo path. We can't
    // intercept the constructed bitbucket.org URL at the network layer, but
    // we *can* verify the URL substitution by passing a full wiremock URL with
    // placeholders embedded — those get replaced before hitting the network.
    Mock::given(method("GET"))
        .and(path("/2.0/repositories/acme/widgets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"full_name": "acme/widgets"})))
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let url = format!("{}/2.0/repositories/{{workspace}}/{{repo}}", server.uri());
    let mut cmd = bb();
    with_token_env(&mut cmd, dir.path(), "tok")
        .args(["-R", "acme/widgets", "api", &url, "--jq", ".full_name"])
        .assert()
        .success()
        .stdout("acme/widgets\n");
}

#[test]
fn bb_api_help_lists_flags() {
    bb().args(["api", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--paginate"))
        .stdout(predicate::str::contains("--jq"))
        .stdout(predicate::str::contains("--field"))
        .stdout(predicate::str::contains("--input"))
        .stdout(predicate::str::contains("--cache"));
}

#[test]
fn bb_api_rejects_slurp_without_paginate() {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = bb();
    with_token_env(&mut cmd, dir.path(), "tok")
        .args(["api", "http://127.0.0.1:1/2.0/user", "--slurp"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--slurp"));
}
