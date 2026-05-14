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
fn unknown_subcommand_is_a_flag_error() {
    bb().arg("nonexistent")
        .assert()
        .failure()
        // clap's default error exit code.
        .code(2);
}
