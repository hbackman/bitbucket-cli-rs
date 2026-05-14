use std::sync::OnceLock;

use tokio::sync::OnceCell;

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
    pub config: OnceCell<crate::config::Config>,
    pub http: OnceCell<reqwest::Client>,
    pub api: OnceCell<crate::api::Client>,
    pub base_repo: OnceCell<crate::bbrepo::BbRepo>,
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
