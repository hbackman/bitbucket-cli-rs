use assert_cmd::Command;
use predicates::prelude::*;

fn bb() -> Command {
    Command::cargo_bin("bb").expect("bb binary built")
}

#[test]
fn version_flag_prints_version() {
    bb().arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("bb "))
        .stdout(predicate::str::contains("commit"))
        .stdout(predicate::str::contains("built"));
}

#[test]
fn version_subcommand_prints_version() {
    bb().arg("version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("bb "));
}

#[test]
fn help_lists_core_subcommands() {
    bb().arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("auth"))
        .stdout(predicate::str::contains("repo"))
        .stdout(predicate::str::contains("pr"))
        .stdout(predicate::str::contains("api"))
        .stdout(predicate::str::contains("browse"))
        .stdout(predicate::str::contains("config"))
        .stdout(predicate::str::contains("version"));
}

#[test]
fn pr_list_stub_returns_not_implemented() {
    bb().args(["pr", "list"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("not yet implemented"));
}

#[test]
fn auth_subcommand_lists_verbs() {
    bb().args(["auth", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("login"))
        .stdout(predicate::str::contains("logout"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("token"))
        .stdout(predicate::str::contains("switch"))
        .stdout(predicate::str::contains("setup-git"))
        .stdout(predicate::str::contains("git-credential"));
}

#[test]
fn auth_status_when_not_logged_in_exits_4() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut cmd = bb();
    cmd.env("BB_CONFIG_DIR", dir.path())
        .env_remove("BB_TOKEN")
        .env_remove("BITBUCKET_TOKEN")
        .args(["auth", "status"]);
    cmd.assert()
        .failure()
        .code(4)
        .stderr(predicate::str::contains("not logged in"));
}

#[test]
fn unknown_subcommand_is_a_flag_error() {
    bb().arg("nonexistent")
        .assert()
        .failure()
        // clap's default error exit code.
        .code(2);
}
