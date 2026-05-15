use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::sync::OnceCell;

use crate::auth::{AuthSource, Browser, DefaultBrowser, KeyringBackend, SystemKeyring};
use crate::bbrepo::BbRepo;
use crate::cli::prompter::{DialoguerPrompter, Prompter};
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
    pub http: OnceLock<reqwest::Client>,
    pub api: OnceCell<crate::api::Client>,
    pub base_repo: OnceCell<BbRepo>,

    /// Browser launcher. Tests pre-seed with `RecordingBrowser`; production reads
    /// the default impl that shells out to `webbrowser`.
    pub browser: OnceLock<Arc<dyn Browser>>,

    /// OS keyring backend. Tests pre-seed with `MemKeyring`.
    pub keyring: OnceLock<Arc<dyn KeyringBackend>>,

    /// Interactive prompter. Tests pre-seed with `MockPrompter`; production
    /// shells out to `dialoguer`.
    pub prompter: OnceLock<Arc<dyn Prompter>>,
}

impl Context {
    pub fn from_env() -> Self {
        Self {
            io: IoStreams::system(),
            build: BuildInfo::from_env(),
            repo_override: OnceLock::new(),
            config: OnceCell::new(),
            http: OnceLock::new(),
            api: OnceCell::new(),
            base_repo: OnceCell::new(),
            browser: OnceLock::new(),
            keyring: OnceLock::new(),
            prompter: OnceLock::new(),
        }
    }

    /// Lazily build a shared `reqwest::Client`. Used by auth/oauth/api paths.
    pub fn http_client(&self) -> &reqwest::Client {
        self.http.get_or_init(default_http_client)
    }

    /// Pluggable browser launcher (tests can pre-seed via `browser.set`).
    pub fn browser(&self) -> Arc<dyn Browser> {
        self.browser
            .get_or_init(|| Arc::new(DefaultBrowser) as Arc<dyn Browser>)
            .clone()
    }

    /// Pluggable keyring backend (tests pre-seed; production uses the OS keyring).
    pub fn keyring(&self) -> Arc<dyn KeyringBackend> {
        self.keyring
            .get_or_init(|| Arc::new(SystemKeyring) as Arc<dyn KeyringBackend>)
            .clone()
    }

    /// Pluggable prompter (tests pre-seed; production uses `dialoguer`).
    pub fn prompter(&self) -> Arc<dyn Prompter> {
        self.prompter
            .get_or_init(|| Arc::new(DialoguerPrompter) as Arc<dyn Prompter>)
            .clone()
    }

    /// Build an [`AuthSource`] from the current configuration + injected dependencies.
    pub async fn auth_source(&self) -> Result<AuthSource, CliError> {
        let cfg = self.config_loaded().await?;
        let http = self.http_client().clone();
        let keyring = self.keyring();
        Ok(AuthSource::new(cfg.hosts(), keyring, http))
    }

    /// Lazily build a [`crate::api::Client`]. Hooked to the default Bitbucket host.
    /// Tests that need to point at a `wiremock` server build their own client
    /// directly via [`crate::api::build_client`] / [`crate::api::Client::with_base`].
    pub async fn api(&self) -> Result<&crate::api::Client, CliError> {
        self.api
            .get_or_try_init(|| async {
                let source = Arc::new(self.auth_source().await?);
                let http = self.http_client().clone();
                let ua = format!("bbk/{} (+{})", self.build.version, BB_HOMEPAGE);
                Ok::<_, CliError>(crate::api::build_client(
                    http,
                    source,
                    crate::config::DEFAULT_HOST,
                    &ua,
                ))
            })
            .await
    }

    /// Lazily load `config.yml` + `hosts.yml`. Cached for the rest of the run.
    pub async fn config_loaded(&self) -> Result<&Config, CliError> {
        self.config
            .get_or_try_init(|| async { Config::load().await.map_err(CliError::Other) })
            .await
    }

    /// Resolve the target repo for the current command. Cached after first call.
    ///
    /// Precedence: `--repo` / `BB_REPO` → `.git/config bbk.default-repo` → `config.yml
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
            http: OnceLock::new(),
            api: OnceCell::new(),
            base_repo: OnceCell::new(),
            browser: OnceLock::new(),
            keyring: OnceLock::new(),
            prompter: OnceLock::new(),
        };
        (ctx, bufs)
    }
}

pub const BB_HOMEPAGE: &str = "https://github.com/hbackman/bitbucket-cli-rs";

fn default_http_client() -> reqwest::Client {
    let ua = format!("bbk/{} (+{})", env!("CARGO_PKG_VERSION"), BB_HOMEPAGE);
    reqwest::Client::builder()
        .user_agent(ua)
        .timeout(Duration::from_secs(30))
        .build()
        .expect("build reqwest client")
}

async fn resolve_base_repo(ctx: &Context) -> Result<BbRepo, CliError> {
    // 1. --repo / BB_REPO (parsed into ctx.repo_override by the root command).
    if let Some(Some(s)) = ctx.repo_override.get() {
        return BbRepo::from_full_name(s).map_err(|e| CliError::Flag(e.to_string()));
    }

    // 2. .git/config bbk.default-repo (per-clone default).
    if let Ok(stored) = crate::git::config_get("bbk.default-repo").await {
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

    // 4-5. git remote origin / upstream. We take the workspace/slug from the
    // remote and ignore its host — bbk only ever talks to the configured
    // Bitbucket host, so a github.com / gitlab.com / etc. remote is treated as
    // a workspace/slug shortcut. The user gets a Bitbucket-side 404 if no such
    // repo exists there.
    let default_host = ctx
        .config_loaded()
        .await
        .map(|c| c.get_or_default("default_host"))
        .unwrap_or_else(|_| crate::config::DEFAULT_HOST.to_string());
    for remote in ["origin", "upstream"] {
        if let Ok(url) = crate::git::remote_url(remote).await {
            if !url.is_empty() {
                if let Ok(mut repo) = BbRepo::parse_remote(&url) {
                    repo.host = default_host.clone();
                    return Ok(repo);
                }
            }
        }
    }

    Err(CliError::Flag(
        "no repository specified — use --repo or `bbk repo set-default`".into(),
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
