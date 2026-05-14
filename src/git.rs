use std::path::PathBuf;

use anyhow::{anyhow, bail, Context as _, Result};
use tokio::process::Command;

/// Read `remote.<name>.url` from the local git config.
pub async fn remote_url(name: &str) -> Result<String> {
    let out = run(&["config", "--get", &format!("remote.{name}.url")]).await?;
    Ok(out.trim().to_string())
}

/// Read an arbitrary local git config key (e.g. `bb.default-repo`).
pub async fn config_get(key: &str) -> Result<String> {
    let out = run(&["config", "--get", key]).await?;
    Ok(out.trim().to_string())
}

/// Read a key from the user's *global* git config (`~/.gitconfig`).
pub async fn config_get_global(key: &str) -> Result<String> {
    let out = run(&["config", "--global", "--get", key]).await?;
    Ok(out.trim().to_string())
}

/// Append a value to a multi-valued *global* git config key.
pub async fn config_add_global(key: &str, value: &str) -> Result<()> {
    run(&["config", "--global", "--add", key, value]).await.map(|_| ())
}

/// Remove every value of a multi-valued *global* git config key. Tolerates
/// "key not found" exits (status 5 in modern git).
pub async fn config_unset_global_all(key: &str) -> Result<()> {
    match run(&["config", "--global", "--unset-all", key]).await {
        Ok(_) => Ok(()),
        Err(e) => {
            // git exits 5 when the key was already absent — that's fine.
            let msg = e.to_string();
            if msg.contains("exit 5") {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

pub async fn current_branch() -> Result<String> {
    let out = run(&["symbolic-ref", "--short", "HEAD"]).await?;
    Ok(out.trim().to_string())
}

pub async fn push(remote: &str, branch: &str) -> Result<()> {
    run(&["push", remote, branch]).await.map(|_| ())
}

pub async fn fetch(remote: &str, refs: &[&str]) -> Result<()> {
    let mut args = vec!["fetch", remote];
    args.extend_from_slice(refs);
    run(&args).await.map(|_| ())
}

pub async fn checkout_branch(branch: &str) -> Result<()> {
    run(&["checkout", branch]).await.map(|_| ())
}

pub async fn create_branch_tracking(branch: &str, remote_ref: &str) -> Result<()> {
    run(&["checkout", "-b", branch, "--track", remote_ref])
        .await
        .map(|_| ())
}

pub async fn repo_root() -> Result<PathBuf> {
    let out = run(&["rev-parse", "--show-toplevel"]).await?;
    Ok(PathBuf::from(out.trim()))
}

async fn run(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .await
        .with_context(|| {
            if which_git().is_none() {
                format!("`git` not found on PATH (running `git {}`)", args.join(" "))
            } else {
                format!("failed to invoke `git {}`", args.join(" "))
            }
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "git {} failed (exit {}): {}",
            args.join(" "),
            output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".into()),
            if stderr.is_empty() {
                "<no stderr>".into()
            } else {
                stderr
            }
        );
    }
    String::from_utf8(output.stdout).map_err(|e| anyhow!("git output was not utf-8: {e}"))
}

fn which_git() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|p| {
            let candidate = p.join("git");
            if candidate.is_file() {
                Some(candidate)
            } else {
                None
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests assume `git` is on PATH and the working directory is a git repo
    // (which is true for this project). They exercise the happy path of the shell-out
    // plumbing without mocking — heavier git-runner abstractions land with later slices.
    #[tokio::test]
    async fn repo_root_returns_a_path() {
        let root = repo_root().await.unwrap();
        assert!(root.is_absolute());
    }

    #[tokio::test]
    async fn current_branch_returns_something() {
        let branch = current_branch().await.unwrap();
        assert!(!branch.is_empty());
    }
}
