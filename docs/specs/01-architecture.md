# 01 — Architecture

**Status:** Draft (Rust pivot — supersedes the earlier Go draft)
**Depends on:** [`00-overview.md`](00-overview.md)
**Slice goal:** Scaffold the binary, command tree, dependency container, and IO streams abstraction. `bb --version` and `bb --help` work. Tests run green. No real commands implemented yet.

## What "done" looks like

```
$ bb --version
bb 0.0.0-dev (commit <sha>, built <date>)

$ bb --help
Bitbucket Cloud command-line tool.

Usage: bb <COMMAND>

Core commands:
  auth        Authenticate with Bitbucket
  repo        Manage repositories
  pr          Manage pull requests

Additional commands:
  api         Make an authenticated request to the Bitbucket REST API
  browse      Open a Bitbucket page in the browser
  config      Manage configuration
  version     Print version information

Options:
  -R, --repo <WORKSPACE/REPO>   Select a repository
  -h, --help                    Print help
  -V, --version                 Print version

$ cargo test
ok
```

Subcommands print "not yet implemented" stubs; their actual surface is filled in by later specs.

## Toolchain

- **Rust 1.75+**, pinned via `rust-toolchain.toml` (`channel = "1.75"`).
- **Edition 2021.**
- **Cargo** for build/test/lint. `cargo-dist` wiring deferred to [`08-distribution.md`](08-distribution.md); a `Makefile` provides convenience targets.

## Crate setup

```toml
# Cargo.toml
[package]
name = "bb"
version = "0.0.0-dev"
edition = "2021"
rust-version = "1.75"
license = "MIT"
description = "Bitbucket Cloud command-line tool"

[[bin]]
name = "bb"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive", "env", "wrap_help"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "fs", "process", "signal", "io-util"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "stream"] }      # used by 04
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"          # config, used by 03
anyhow = "1"
thiserror = "1"
owo-colors = { version = "4", features = ["supports-colors"] }
anstream = "0.6"
url = "2"
directories = "5"           # XDG dirs
oauth2 = "4"                # used by 02
keyring = "3"               # used by 02
jaq-core = "1"              # used by 05
jaq-interpret = "1"
jaq-parse = "1"
jaq-std = "1"

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"
wiremock = "0.6"            # used by 04+ for HTTP fixtures
```

Pin minor versions in `Cargo.toml`, exact versions resolved into `Cargo.lock`. Use the latest stable minor for each.

Some dependencies (`oauth2`, `keyring`, `jaq-*`, `reqwest`) are introduced here even though they aren't *used* until later slices. Pulling them in now keeps the dependency tree stable so subsequent slices don't churn `Cargo.lock` with every addition. If a dep turns out unused, drop it when its slice lands.

## Directory layout

```
bitbucket-cli/
├── src/
│   ├── main.rs                      # entrypoint, tokio main, error printing
│   ├── lib.rs                       # re-exports for integration tests
│   ├── cli/
│   │   ├── mod.rs                   # root Cli struct + dispatch
│   │   ├── auth.rs                  # filled by 02 (stub now)
│   │   ├── repo.rs                  # filled by 07 (stub now)
│   │   ├── pr.rs                    # filled by 06 (stub now)
│   │   ├── api.rs                   # filled by 04 (stub now)
│   │   ├── browse.rs                # filled by 07 (stub now)
│   │   ├── config.rs                # filled by 03 (stub now)
│   │   └── version.rs               # fully implemented
│   ├── context.rs                   # App context (DI container)
│   ├── error.rs                     # CliError, ExitCode, error printing
│   ├── iostreams.rs                 # TTY-aware stdin/stdout/stderr + color
│   ├── git.rs                       # `git` shell-out wrappers
│   ├── bbrepo.rs                    # workspace/slug parsing
│   ├── text.rs                      # pluralize, truncate, indent helpers
│   ├── auth/                        # filled by 02 (mod stub)
│   │   └── mod.rs
│   ├── config/                      # filled by 03 (mod stub)
│   │   └── mod.rs
│   └── api/                         # filled by 04 (mod stub)
│       └── mod.rs
├── tests/                           # integration tests (assert_cmd)
│   └── cli_smoke.rs
├── docs/specs/                      # this directory
├── notes/                           # research notes
├── .github/workflows/ci.yml
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── rustfmt.toml
├── clippy.toml                      # optional, mostly defaults
├── Makefile
├── LICENSE                          # MIT
└── README.md
```

## `src/main.rs`

```rust
use std::process::ExitCode;

use bb::{cli, context::Context, error};

#[tokio::main]
async fn main() -> ExitCode {
    let ctx = Context::from_env();
    match cli::run(ctx).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => error::report(err),
    }
}
```

Build metadata (`version`, `commit`, `date`) is injected via a `build.rs` (described under "Build metadata" below) so the version command can read it from compile-time env vars. No `-ldflags` analog needed.

## `src/lib.rs`

```rust
pub mod cli;
pub mod context;
pub mod error;
pub mod iostreams;
pub mod git;
pub mod bbrepo;
pub mod text;

pub mod auth;
pub mod config;
pub mod api;
```

Exposing modules via `lib.rs` lets `tests/` reach internals without going through the binary boundary.

## App context (`src/context.rs`)

The Rust analog of gh's `Factory` pattern. Lazily constructed dependencies live in `OnceCell`s.

```rust
use std::sync::OnceLock;
use tokio::sync::OnceCell;

use crate::iostreams::IoStreams;

#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub version: &'static str,
    pub commit: &'static str,
    pub date: &'static str,
}

pub struct Context {
    pub io: IoStreams,
    pub build: BuildInfo,
    pub repo_override: OnceLock<Option<String>>,  // populated from --repo / BB_REPO

    // Lazily constructed; populated by their owning slices.
    pub config: OnceCell<crate::config::Config>,           // 03
    pub http:   OnceCell<reqwest::Client>,                 // 04
    pub api:    OnceCell<crate::api::Client>,              // 04
    pub base_repo: OnceCell<crate::bbrepo::BbRepo>,        // 03
}

impl Context {
    pub fn from_env() -> Self { /* IoStreams::system(), BuildInfo from env!(...) */ }
}

#[cfg(test)]
impl Context {
    pub fn test() -> (Self, crate::iostreams::TestBuffers) { /* IoStreams::test() */ }
}
```

Rules — direct translations of the gh-style discipline:

- **No `println!`/`eprintln!` in command code.** All output flows through `ctx.io`. The macros from `anstream` (`println!` / `eprintln!` via `anstream::println!` etc.) write to the IoStreams writers, never to `std::io::stdout` directly.
- **No `reqwest::Client::new()` outside the context.** Commands ask `ctx.http()` (or `ctx.api()`), which lazily builds the shared client.
- **No global statics for state.** Build info is `'static` and that's fine; everything else lives on `Context`.

## `IoStreams` (`src/iostreams.rs`)

```rust
use std::io::{self, IsTerminal, Read, Write};

pub struct IoStreams {
    inner: IoInner,
    color_enabled: bool,
    is_stdout_tty: bool,
    is_stderr_tty: bool,
    is_stdin_tty: bool,
}

enum IoInner {
    System,                    // wraps real stdin/stdout/stderr
    Test(TestBuffers),         // in-memory buffers for tests
}

#[derive(Default, Clone)]
pub struct TestBuffers {
    pub stdin:  std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    pub stdout: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    pub stderr: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

impl IoStreams {
    pub fn system() -> Self { /* IsTerminal probe + NO_COLOR / CLICOLOR_FORCE handling */ }
    pub fn test()  -> (Self, TestBuffers) { /* in-memory buffers */ }

    pub fn is_stdout_tty(&self) -> bool { self.is_stdout_tty }
    pub fn color_enabled(&self) -> bool { self.color_enabled }

    pub fn out<'a>(&'a mut self) -> Box<dyn Write + 'a> { /* ... */ }
    pub fn err<'a>(&'a mut self) -> Box<dyn Write + 'a> { /* ... */ }
}
```

- TTY detection uses `std::io::IsTerminal` (stable since 1.70).
- Color enablement honors `NO_COLOR`, `CLICOLOR_FORCE`, and `--color={auto,always,never}` (top-level flag wired in spec 05).
- Color helpers are thin re-exports of `owo-colors` traits (`Bold`, `Red`, etc.). When `color_enabled == false`, the writer strips ANSI sequences via `anstream::adapter::strip_str`.
- Pager support (`StartPager` / `StopPager`) is post-MVP. Leave a `pub fn start_pager()` stub returning `Ok(())`.

## Root command (`src/cli/mod.rs`)

Uses `clap` derive. Subcommands live in sibling modules.

```rust
use clap::{Parser, Subcommand};

use crate::context::Context;

#[derive(Parser, Debug)]
#[command(
    name = "bb",
    version,
    about = "Bitbucket Cloud command-line tool",
    long_about = None,
    propagate_version = true,
    disable_help_subcommand = true,
)]
pub struct Cli {
    /// Select a repository using the WORKSPACE/REPO format.
    #[arg(short = 'R', long = "repo", global = true, env = "BB_REPO")]
    pub repo: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Authenticate with Bitbucket
    #[command(help_heading = "Core commands")]
    Auth(super::auth_cmd::AuthArgs),

    /// Manage repositories
    #[command(help_heading = "Core commands")]
    Repo(super::repo_cmd::RepoArgs),

    /// Manage pull requests
    #[command(help_heading = "Core commands")]
    Pr(super::pr_cmd::PrArgs),

    /// Make an authenticated request to the Bitbucket REST API
    #[command(help_heading = "Additional commands")]
    Api(super::api_cmd::ApiArgs),

    /// Open a Bitbucket page in the browser
    #[command(help_heading = "Additional commands")]
    Browse(super::browse_cmd::BrowseArgs),

    /// Manage configuration
    #[command(help_heading = "Additional commands")]
    Config(super::config_cmd::ConfigArgs),

    /// Print version information
    #[command(help_heading = "Additional commands")]
    Version,
}

pub async fn run(mut ctx: Context) -> Result<(), crate::error::CliError> {
    let cli = Cli::parse();
    let _ = ctx.repo_override.set(cli.repo);
    match cli.command {
        Command::Auth(a)    => crate::cli::auth::run(a, &mut ctx).await,
        Command::Repo(a)    => crate::cli::repo::run(a, &mut ctx).await,
        Command::Pr(a)      => crate::cli::pr::run(a, &mut ctx).await,
        Command::Api(a)     => crate::cli::api::run(a, &mut ctx).await,
        Command::Browse(a)  => crate::cli::browse::run(a, &mut ctx).await,
        Command::Config(a)  => crate::cli::config::run(a, &mut ctx).await,
        Command::Version    => crate::cli::version::run(&mut ctx).await,
    }
}
```

Until subsequent specs land, each subcommand module exports `pub struct XArgs;` and `pub async fn run(_: XArgs, _: &mut Context) -> Result<(), CliError> { Err(CliError::NotImplemented) }`.

(In code, prefer the shorter form `use super::auth as auth_cmd;` rather than the disambiguated names shown above; the spec uses qualified names only for clarity.)

## `--repo` resolution wiring

`clap` parses `--repo` / `BB_REPO` into `Cli::repo`. The root `run()` stuffs it into `ctx.repo_override`. Commands that need the repo call `ctx.base_repo().await?`, which:

1. Returns the `--repo` value parsed via `BbRepo::from_full_name` if set.
2. Otherwise reads `git config --get remote.origin.url` (via `crate::git`) and parses with `BbRepo::from_url`.
3. Caches the resolved repo in `ctx.base_repo`.

Full details in [`03-configuration.md`](03-configuration.md) (still Go-flavored at the time of writing — see the reconciliation note in 00).

## Error types (`src/error.rs`)

```rust
use std::process::ExitCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    /// Bad CLI args — clap has already printed usage; we just exit nonzero.
    #[error("invalid arguments: {0}")]
    Flag(String),

    /// Error was already printed to stderr; exit nonzero quietly.
    #[error("(silent)")]
    Silent,

    /// Authentication failed or missing — exit code 4.
    #[error("authentication: {0}")]
    Auth(String),

    /// Resource not found — exit code 3.
    #[error("not found: {0}")]
    NotFound(String),

    /// Stub for not-yet-implemented commands.
    #[error("not yet implemented")]
    NotImplemented,

    /// Catch-all wrapping an inner error.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub fn report(err: CliError) -> ExitCode { /* print to stderr, return appropriate code */ }
```

Exit-code mapping mirrors [`05-output.md`](05-output.md):

| Variant            | Exit code |
| ------------------ | --------- |
| `Flag`             | 2         |
| `NotFound`         | 3         |
| `Auth`             | 4         |
| `NotImplemented`   | 1         |
| `Silent` / `Other` | 1         |

## Logging

No structured logger in MVP. Status messages go via `writeln!(ctx.io.err(), ...)`. If a logger is added later, route it through IoStreams.

`BB_DEBUG=1` enables HTTP request/response dumping in the API client — wired in [`04-api-client.md`](04-api-client.md).

## Git wrapper (`src/git.rs`)

Shell out to `git` via `tokio::process::Command`. Minimum surface needed by later slices:

```rust
pub async fn remote_url(name: &str) -> anyhow::Result<String>;       // git config --get remote.<name>.url
pub async fn current_branch() -> anyhow::Result<String>;
pub async fn push(remote: &str, branch: &str) -> anyhow::Result<()>;
pub async fn fetch(remote: &str, refs: &[&str]) -> anyhow::Result<()>;
pub async fn checkout_branch(branch: &str) -> anyhow::Result<()>;
pub async fn create_branch_tracking(branch: &str, remote_ref: &str) -> anyhow::Result<()>;
pub async fn repo_root() -> anyhow::Result<std::path::PathBuf>;      // git rev-parse --show-toplevel
```

All functions return a wrapped error if `git` is missing on PATH. Unit tests stub via a `GitRunner` trait if needed; for the scaffold slice, only the function signatures and shell-out happy-path are required.

## Repo identifier (`src/bbrepo.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BbRepo {
    pub workspace: String,
    pub slug: String,
    pub host: String,        // "bitbucket.org" for MVP
}

impl BbRepo {
    pub fn new(workspace: impl Into<String>, slug: impl Into<String>) -> Self;
    pub fn with_host(workspace: impl Into<String>, slug: impl Into<String>, host: impl Into<String>) -> Self;
    pub fn from_full_name(s: &str) -> anyhow::Result<Self>;          // "workspace/slug"
    pub fn from_url(u: &url::Url) -> anyhow::Result<Self>;           // https://bitbucket.org/...
    pub fn parse_remote(remote: &str) -> anyhow::Result<Self>;       // dispatches URL vs ssh form
}

impl std::fmt::Display for BbRepo { /* workspace/slug */ }
```

`parse_remote` handles:

- `https://bitbucket.org/workspace/repo.git`
- `https://bitbucket.org/workspace/repo`
- `git@bitbucket.org:workspace/repo.git`
- `ssh://git@bitbucket.org/workspace/repo.git`

Unit tests in the same file (`#[cfg(test)] mod tests`).

## Build metadata (`build.rs`)

`cargo` exposes `CARGO_PKG_VERSION` for free. For commit/date we use a small `build.rs`:

```rust
// build.rs
use std::process::Command;

fn main() {
    let commit = Command::new("git").args(["rev-parse", "--short", "HEAD"])
        .output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    // ...or shell out to `date -u +%Y-%m-%d` to avoid pulling in chrono just for this.

    println!("cargo:rustc-env=BB_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=BB_BUILD_DATE={date}");
    println!("cargo:rerun-if-changed=.git/HEAD");
}
```

`main.rs` reads them via `env!("BB_BUILD_COMMIT")` / `env!("BB_BUILD_DATE")`. To avoid pulling in `chrono` just for a date, the scaffold uses `std::process::Command` to call `date -u +%Y-%m-%d` (with `"unknown"` fallback).

## Testing approach

- Stdlib `cargo test`.
- **Unit tests** live in `#[cfg(test)] mod tests { ... }` inside each source file.
- **Integration tests** under `tests/` use `assert_cmd` to invoke the compiled binary and `predicates` for assertions on stdout/stderr.
- HTTP fixtures (from spec 04 onward) use `wiremock` with per-test mock servers; recorded JSON lives under `tests/fixtures/`.
- A `TestBuffers`-backed `Context` lets command-level tests run without `assert_cmd` overhead when only output assertions are needed.

## Linting & formatting

- `rustfmt.toml`: keep defaults; set `edition = "2021"`, `max_width = 100`.
- `clippy`: CI runs `cargo clippy --all-targets --all-features -- -D warnings`. Allow-list pragmas only where justified inline.

## Makefile targets

```
make build        # cargo build --release
make test         # cargo test --all
make lint         # cargo clippy --all-targets -- -D warnings && cargo fmt --check
make fmt          # cargo fmt
make install      # cargo install --path .
make clean        # cargo clean
```

(Cargo handles most of this directly; the Makefile is for muscle-memory parity with `make build`.)

## Concrete deliverables for this slice

Create the files listed under "Directory layout" with:

1. `Cargo.toml` with the dependencies listed above.
2. `rust-toolchain.toml` pinning the stable channel (1.75+).
3. `build.rs` emitting `BB_BUILD_COMMIT` and `BB_BUILD_DATE`.
4. `src/main.rs` with the tokio entrypoint.
5. `src/lib.rs` re-exporting modules.
6. `src/context.rs` with `Context`, `BuildInfo`, lazy fields.
7. `src/error.rs` with `CliError`, exit-code mapping, `report()`.
8. `src/iostreams.rs` with `system()` / `test()` constructors and `TestBuffers`.
9. `src/cli/mod.rs` with the clap root + dispatch.
10. `src/cli/version.rs` — fully implemented; prints version, commit, date.
11. Stub `src/cli/{auth,repo,pr,api,browse,config}.rs` whose `run()` returns `Err(CliError::NotImplemented)`.
12. Stub `src/{auth,config,api}/mod.rs` so other modules can name the types they own (empty `pub struct Client;` / `pub struct Config;` placeholders).
13. `src/git.rs` with the wrapper functions (real `tokio::process::Command` calls).
14. `src/bbrepo.rs` with parsing + unit tests.
15. `tests/cli_smoke.rs` — `assert_cmd` tests for `--version`, `--help`, and a stubbed subcommand error.
16. `Makefile`, `rustfmt.toml`, `.github/workflows/ci.yml`.
17. `LICENSE` (MIT), `README.md` (minimal — full README written when 0.1.0 ships).

## Acceptance criteria

- `cargo build --release` produces `./target/release/bb`.
- `./target/release/bb --version` prints version, commit, and date.
- `./target/release/bb --help` lists `auth`, `repo`, `pr`, `api`, `browse`, `config`, `version` under the right help-heading groups.
- `./target/release/bb pr list` exits nonzero with "not yet implemented".
- `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
- CI workflow runs both on PRs to `main`.

## Open questions

- Whether to commit `Cargo.lock`. **Default: yes** (binary crate, reproducible builds).
- Whether to vendor deps for offline builds. **Default: no**; rely on the registry cache.
