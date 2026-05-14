use std::sync::OnceLock;

use tokio::sync::OnceCell;

use crate::bbrepo::BbRepo;
use crate::config::Config;
use crate::error::CliError;
use crate::iostreams::IoStreams;

#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub version: &'static str,
    pub commit: &'static str,
    pub date: &'static str,
}

impl BuildInfo {
    pub fn from_env() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
            commit: env!("BB_BUILD_COMMIT"),
            date: env!("BB_BUILD_DATE"),
        }
    }
}

/// Application context — the Rust analog of gh's `Factory`.
///
/// Holds the IO streams, build metadata, and lazily-constructed dependencies.
/// Every command receives `&mut Context` and reaches all side-effecting machinery
/// through it; nothing in the command tree should touch `std::io::stdout` or
/// build its own `reqwest::Client`.
pub struct Context {
    pub io: IoStreams,
    pub build: BuildInfo,

    /// `--repo` / `BB_REPO` value, set once by the root command dispatch.
    pub repo_override: OnceLock<Option<String>>,

    /// Lazily constructed dependencies. Each is populated by the slice that owns it.
    pub config: OnceCell<Config>,
    pub http: OnceCell<reqwest::Client>,
    pub api: OnceCell<crate::api::Client>,
    pub base_repo: OnceCell<BbRepo>,
}

impl Context {
    pub fn from_env() -> Self {
        Self {
            io: IoStreams::system(),
            build: BuildInfo::from_env(),
            repo_override: OnceLock::new(),
            config: OnceCell::new(),
            http: OnceCell::new(),
            api: OnceCell::new(),
            base_repo: OnceCell::new(),
        }
    }

    /// Lazily load `config.yml` + `hosts.yml`. Cached for the rest of the run.
    pub async fn config_loaded(&self) -> Result<&Config, CliError> {
        self.config
            .get_or_try_init(|| async { Config::load().await.map_err(CliError::Other) })
            .await
    }

    /// Resolve the target repo for the current command. Cached after first call.
    ///
    /// Precedence: `--repo` / `BB_REPO` → `.git/config bb.default-repo` → `config.yml
    /// default_repo` → `git remote origin` → `git remote upstream`.
    pub async fn base_repo(&self) -> Result<&BbRepo, CliError> {
        self.base_repo
            .get_or_try_init(|| async { resolve_base_repo(self).await })
            .await
    }

    #[cfg(test)]
    pub fn test() -> (Self, crate::iostreams::TestBuffers) {
        let (io, bufs) = IoStreams::test();
        let ctx = Self {
            io,
            build: BuildInfo {
                version: "test",
                commit: "test",
                date: "test",
            },
            repo_override: OnceLock::new(),
            config: OnceCell::new(),
            http: OnceCell::new(),
            api: OnceCell::new(),
            base_repo: OnceCell::new(),
        };
        (ctx, bufs)
    }
}

async fn resolve_base_repo(ctx: &Context) -> Result<BbRepo, CliError> {
    // 1. --repo / BB_REPO (parsed into ctx.repo_override by the root command).
    if let Some(Some(s)) = ctx.repo_override.get() {
        return BbRepo::from_full_name(s).map_err(|e| CliError::Flag(e.to_string()));
    }

    // 2. .git/config bb.default-repo (per-clone default).
    if let Ok(stored) = crate::git::config_get("bb.default-repo").await {
        if !stored.is_empty() {
            return BbRepo::from_full_name(&stored).map_err(|e| CliError::Flag(e.to_string()));
        }
    }

    // 3. config.yml default_repo (global fallback).
    let cfg = ctx.config_loaded().await?;
    if let Some(v) = cfg.get("default_repo") {
        if !v.is_empty() {
            return BbRepo::from_full_name(&v).map_err(|e| CliError::Flag(e.to_string()));
        }
    }

    // 4-5. git remote origin / upstream.
    for remote in ["origin", "upstream"] {
        if let Ok(url) = crate::git::remote_url(remote).await {
            if !url.is_empty() {
                if let Ok(repo) = BbRepo::parse_remote(&url) {
                    return Ok(repo);
                }
            }
        }
    }

    Err(CliError::Flag(
        "no repository specified — use --repo or `bb repo set-default`".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn base_repo_uses_repo_override() {
        let (ctx, _bufs) = Context::test();
        ctx.repo_override.set(Some("acme/widgets".into())).unwrap();
        let repo = ctx.base_repo().await.unwrap();
        assert_eq!(repo.workspace, "acme");
        assert_eq!(repo.slug, "widgets");
    }

    #[tokio::test]
    async fn base_repo_errors_on_malformed_override() {
        let (ctx, _bufs) = Context::test();
        ctx.repo_override.set(Some("nope".into())).unwrap();
        let err = ctx.base_repo().await.unwrap_err();
        assert!(matches!(err, CliError::Flag(_)));
    }
}
