//! Integration tests for `bb config get|set|list`. Each test points `BB_CONFIG_DIR`
//! at a fresh tempdir so they neither read nor write the developer's real config.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn bb_in(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("bb").expect("bb binary built");
    cmd.env("BB_CONFIG_DIR", dir.path());
    // Defensive: ensure we never inherit a real $BB_REPO or $BB_HOST from CI.
    cmd.env_remove("BB_REPO");
    cmd.env_remove("BB_HOST");
    cmd
}

#[test]
fn set_then_get_round_trip() {
    let dir = TempDir::new().unwrap();

    bb_in(&dir)
        .args(["config", "set", "editor", "code -w"])
        .assert()
        .success();

    bb_in(&dir)
        .args(["config", "get", "editor"])
        .assert()
        .success()
        .stdout("code -w\n");
}

#[test]
fn get_unknown_key_exits_1() {
    let dir = TempDir::new().unwrap();
    bb_in(&dir)
        .args(["config", "get", "bogus"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("unknown key 'bogus'"));
}

#[test]
fn set_rejects_invalid_git_protocol() {
    let dir = TempDir::new().unwrap();
    bb_in(&dir)
        .args(["config", "set", "git_protocol", "carrier-pigeon"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("git_protocol"));
}

#[test]
fn set_rejects_unknown_key() {
    let dir = TempDir::new().unwrap();
    bb_in(&dir)
        .args(["config", "set", "bogus", "value"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown key"));
}

#[test]
fn list_includes_defaults() {
    let dir = TempDir::new().unwrap();
    bb_in(&dir)
        .args(["config", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("git_protocol: https"))
        .stdout(predicate::str::contains("default_host: bitbucket.org"))
        .stdout(predicate::str::contains("prompt: enabled"));
}

#[test]
fn list_reflects_user_overrides() {
    let dir = TempDir::new().unwrap();
    bb_in(&dir)
        .args(["config", "set", "git_protocol", "ssh"])
        .assert()
        .success();
    bb_in(&dir)
        .args(["config", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("git_protocol: ssh"));
}

#[test]
fn host_scoped_set_writes_hosts_yml() {
    let dir = TempDir::new().unwrap();
    bb_in(&dir)
        .args([
            "config",
            "set",
            "--host",
            "bitbucket.org",
            "active_user",
            "hbackman",
        ])
        .assert()
        .success();

    let raw = std::fs::read_to_string(dir.path().join("hosts.yml")).unwrap();
    assert!(raw.contains("bitbucket.org"), "got: {raw}");
    assert!(raw.contains("active_user"), "got: {raw}");
    assert!(raw.contains("hbackman"), "got: {raw}");

    bb_in(&dir)
        .args(["config", "get", "--host", "bitbucket.org", "active_user"])
        .assert()
        .success()
        .stdout("hbackman\n");
}

#[test]
fn pr_list_with_repo_override_targets_that_repo() {
    // `--repo` should propagate through dispatch and reach the API layer, which
    // fails with an auth error in an unconfigured config dir — exit code 4, not 2.
    let dir = TempDir::new().unwrap();
    bb_in(&dir)
        .args(["-R", "acme/widgets", "pr", "list"])
        .assert()
        .failure()
        .code(4)
        .stderr(predicate::str::contains("not logged in"));
}

#[test]
fn bb_repo_env_var_routes_through_dispatch() {
    let dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("bb").expect("bb binary built");
    cmd.env("BB_CONFIG_DIR", dir.path())
        .env("BB_REPO", "acme/widgets")
        .env_remove("BB_TOKEN")
        .env_remove("BITBUCKET_TOKEN")
        .args(["pr", "list"]);
    cmd.assert()
        .failure()
        .code(4)
        .stderr(predicate::str::contains("not logged in"));
}
