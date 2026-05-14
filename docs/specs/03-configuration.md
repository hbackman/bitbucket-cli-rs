# 03 — Configuration

**Status:** Draft (Rust)
**Depends on:** [`01-architecture.md`](01-architecture.md)
**Slice goal:** Config files + env-var precedence are in place. `bb config get/set/list` works. Repo resolution (`--repo`, `BB_REPO`, `git remote`) returns the right `BbRepo` from every interesting starting state.

## Files

Two YAML files, both under `$XDG_CONFIG_HOME/bb/` (defaults to `~/.config/bb/` on macOS/Linux, `%AppData%\bb\` on Windows). Permissions: `0600` on Unix; Windows uses ACL defaults.

- `config.yml` — user preferences. Owned by the user; we never edit it without an explicit command.
- `hosts.yml` — per-host auth state. Owned by `bb`; users may inspect but generally don't edit by hand. Schema defined in [`02-authentication.md`](02-authentication.md).

Locate the directory via the `directories` crate (`ProjectDirs::from("", "", "bb")`) and override via `BB_CONFIG_DIR`.

### `config.yml` schema

```yaml
# Default Bitbucket host. Overridden by --hostname or BB_HOST.
default_host: bitbucket.org

# Default protocol for cloning, setting remotes, etc.
git_protocol: https            # "https" | "ssh"

# Editor for PR bodies, comments, etc.
# Resolved as: --body-file > stdin > $BB_EDITOR > editor here > $VISUAL > $EDITOR > vim
editor: ""

# Pager. Empty disables paging. "less -R" recommended.
pager: ""

# Browser command. Empty uses platform default (via `webbrowser` crate).
browser: ""

# Per-command prompt suppression. Equivalent to --no-prompt on every invocation.
prompt: enabled                # "enabled" | "disabled"

# Aliases (post-MVP — accept the key but don't act on it yet)
aliases: {}
```

Missing keys take their defaults. Unknown keys are preserved on write (so a future `bb` version's keys aren't clobbered): we round-trip through `serde_yaml::Value` rather than a strongly-typed struct on write.

### Reading config (`src/config/mod.rs`)

```rust
#[derive(Debug, Clone, Default)]
pub struct Config {
    data: serde_yaml::Mapping,           // preserves unknown keys
    path: PathBuf,
    aliases: BTreeMap<String, String>,
    hosts: Arc<RwLock<Hosts>>,
}

impl Config {
    /// Read both files. Missing files are fine — returns an empty config.
    pub async fn load() -> Result<Self> { /* ... */ }

    /// Returns the raw stored value, or `None`. Unknown keys are allowed on read.
    pub fn get(&self, key: &str) -> Option<&str>;

    /// Returns the value with hardcoded defaults applied.
    pub fn get_or_default(&self, key: &str) -> &str;

    /// Validates and persists. `set` is the only public mutator.
    pub async fn set(&mut self, key: &str, value: &str) -> Result<()>;

    pub fn aliases(&self) -> &BTreeMap<String, String>;
    pub fn hosts(&self) -> Arc<RwLock<Hosts>>;
}
```

Loaded once and cached in `Context.config` (a `tokio::sync::OnceCell`). Commands that mutate config (`bb config set`, `bb repo set-default`) write through and refresh the cell.

## Environment variables

Documented in `bb help environment` (a generated docs page later; for now, hardcode the table in `bb config --help`).

| Var | Meaning |
| --- | --- |
| `BB_TOKEN` | Override stored auth token (highest precedence). |
| `BITBUCKET_TOKEN` | Alias for `BB_TOKEN`. |
| `BB_HOST` | Override host for the current invocation. |
| `BB_REPO` | Override `--repo` value: `WORKSPACE/SLUG`. |
| `BB_OAUTH_CLIENT_ID` | Override embedded OAuth client_id. |
| `BB_OAUTH_CLIENT_SECRET` | Override embedded OAuth client_secret. |
| `BB_CONFIG_DIR` | Override the config directory. |
| `BB_EDITOR` | Editor for prompts (highest editor precedence). |
| `BB_PAGER` | Pager (overrides config). |
| `BB_BROWSER` | Browser (overrides config). |
| `BB_DEBUG` | When set to `1` or `true`, dump HTTP requests/responses to stderr. |
| `BB_NO_UPDATE_NOTIFIER` | Disable the "new version available" check. |
| `NO_COLOR` | Disable color output (standard). |
| `CLICOLOR` | Standard color hint. |
| `CLICOLOR_FORCE` | Force color even when not a TTY. |
| `XDG_CONFIG_HOME`, `XDG_CACHE_HOME` | Standard XDG paths. |

## Precedence chains

### Auth token resolution
1. `BB_TOKEN`
2. `BITBUCKET_TOKEN`
3. OS keyring entry for `(host, active_user)`
4. Plaintext entry in `hosts.yml` for `(host, active_user)`

### Host resolution
1. `--hostname` flag (where applicable)
2. `BB_HOST` env
3. `config.yml` `default_host`
4. Hardcoded `bitbucket.org`

### Repo resolution (the important one)
1. `--repo WORKSPACE/SLUG` flag (parsed by clap into `Cli::repo`)
2. `BB_REPO` env (also reaches `Cli::repo` via `env = "BB_REPO"`)
3. `bb repo set-default` value stored under `bb.default-repo` in `.git/config` (per-clone) — see "default repo location" below.
4. `default_repo` in `config.yml` (global fallback)
5. Parsed from `git remote get-url origin` if inside a git repo
6. Parsed from `git remote get-url upstream` as fallback
7. Error: `CliError::Flag("no repository specified — use --repo or `bb repo set-default`")`

### Editor resolution
1. `--body-file -` (stdin)
2. `BB_EDITOR`
3. `config.yml` `editor`
4. `$VISUAL`
5. `$EDITOR`
6. `vim` (macOS/Linux), `notepad` (Windows)

## `bb config` subcommands

```
bb config get KEY [--host HOST]
bb config set KEY VALUE [--host HOST]
bb config list [--host HOST]
bb config clear-cache                  # future
```

When `--host` is provided, read/write under `hosts.yml`'s `<host>:` block. Otherwise read/write `config.yml`.

Behavior:
- `bb config get editor` → prints value (or empty line if unset). Exit 0.
- `bb config get bogus` → prints to stderr "unknown key 'bogus'", exit 1.
- `bb config set editor "code -w"` → writes config, prints nothing on success.
- `bb config list` → YAML of the effective config (including defaults).

Validation:
- `git_protocol` must be `https` or `ssh`.
- `prompt` must be `enabled` or `disabled`.
- Unknown keys rejected on `set`, accepted on `get` (read passthrough — but stripped from `list`).

## Repo resolution (`src/bbrepo.rs` + helper on `Context`)

Three pieces:

### 1. Identifier type — already in [`01-architecture.md`](01-architecture.md)

```rust
pub struct BbRepo { pub workspace: String, pub slug: String, pub host: String }
impl BbRepo {
    pub fn from_full_name(s: &str) -> Result<Self>;
    pub fn from_url(u: &url::Url) -> Result<Self>;
    pub fn parse_remote(remote: &str) -> Result<Self>;
}
```

### 2. URL parser

Already covered in spec 01. This slice only needs to wire it into the resolution helper.

### 3. The repo-resolution helper

```rust
impl Context {
    /// Resolve the target repo for the current command.
    /// Cached in `self.base_repo` on first call.
    pub async fn base_repo(&self) -> Result<&BbRepo, CliError> {
        self.base_repo
            .get_or_try_init(|| async { resolve_base_repo(self).await })
            .await
    }
}

async fn resolve_base_repo(ctx: &Context) -> Result<BbRepo, CliError> {
    // 1. --repo / BB_REPO (already parsed into ctx.repo_override)
    if let Some(Some(s)) = ctx.repo_override.get() {
        return BbRepo::from_full_name(s).map_err(into_flag);
    }

    let cfg = ctx.config_loaded().await?;

    // 2. .git/config bb.default-repo
    if let Ok(remote) = git::config_get("bb.default-repo").await {
        if !remote.is_empty() {
            return BbRepo::from_full_name(&remote).map_err(into_flag);
        }
    }

    // 3. config.yml default_repo
    if let Some(v) = cfg.get("default_repo") {
        if !v.is_empty() {
            return BbRepo::from_full_name(v).map_err(into_flag);
        }
    }

    // 4-5. git remote origin / upstream
    for remote in ["origin", "upstream"] {
        if let Ok(url) = git::remote_url(remote).await {
            if let Ok(repo) = BbRepo::parse_remote(&url) {
                return Ok(repo);
            }
        }
    }

    Err(CliError::Flag("no repository specified — use --repo or `bb repo set-default`".into()))
}
```

`ctx.config_loaded()` lazily fills `ctx.config`. `git::config_get` is added to `src/git.rs` (shell-out to `git config --get bb.default-repo`).

## Default-repo location

We store the per-clone default in `.git/config` under `bb.default-repo`, matching gh's strategy. Falls back to a global `default_repo:` in `config.yml` when not inside a git repo.

## `bb repo set-default`

```
$ bb repo set-default
? Which repository should be the default? [list of remotes]
✓ Set default repo for this directory to workspace/repo
```

Implementation lives at `src/cli/repo/set_default.rs` (see [`07-commands-repo.md`](07-commands-repo.md)). It writes `bb.default-repo = workspace/slug` via `git config --local`. Flags: positional `WORKSPACE/SLUG`, `--view` to print the current value, `--unset`.

## Config file lifecycle

- On first read, `Config::load()` reads both `config.yml` and `hosts.yml` if present. Missing files are fine — returns an empty config.
- Writes are atomic: write to `<file>.tmp` in the same directory, fsync, rename over the target.
- Permissions: explicit `chmod 0600` on Unix after write; skipped on Windows.
- Never write `config.yml` from a command unless the user invoked `bb config set` or `bb repo set-default` (or similar explicit action).
- `hosts.yml` is written by `bb auth {login,logout,refresh,switch}` and by the token refresh path.

## File layout

```
src/config/
├── mod.rs                # Config struct, load(), aliases(), hosts()
├── yaml.rs               # serde_yaml read/write with atomic rename + chmod
├── hosts.rs              # Hosts struct (shared with src/auth)
└── tests.rs              # unit tests collected here

src/cli/config/
├── mod.rs                # `bb config ...` clap subcommand tree
├── get.rs
├── set.rs
└── list.rs
```

The bare `pub struct Config;` placeholder from spec 01 is replaced by this slice.

## Tests to write

- YAML round-trip preserving unknown keys.
- Atomic write: a crash mid-write doesn't truncate the existing file (use `tempfile::NamedTempFile::persist`).
- Permission check: file written with mode 0600 (Unix only).
- Env-var precedence: `BB_HOST` beats `config.yml`, `--hostname` beats both.
- Repo resolution: every URL form parses; missing remote → fallback to `upstream`; both missing → `CliError::Flag`.
- `bb config set` rejects invalid `git_protocol` values.
- `bb config get` on an unknown key exits 1 with the right message.

## Concrete deliverables

1. `src/config/{mod,yaml,hosts}.rs` with full impl + tests.
2. `Context::base_repo()` helper in `src/context.rs` (replaces the `OnceCell<BbRepo>` placeholder from spec 01).
3. `src/cli/config/{get,set,list}.rs` subcommands.
4. Root command dispatch wiring (already in place from spec 01).
5. New helper `git::config_get` in `src/git.rs`.
6. Add the `BB_CONFIG_DIR` override path to `Config::load()`.

## Acceptance criteria

- `bb config set editor "code -w"` then `bb config get editor` → prints `code -w`.
- `bb config list` shows the merged effective config.
- In a git clone with `origin = git@bitbucket.org:foo/bar.git`, `bb pr list` (once implemented) targets `foo/bar`. Verified via a test that injects a fake `git::remote_url`.
- `BB_REPO=other/repo bb pr list` targets `other/repo` regardless of `git remote`.
- `bb -R x/y pr list` targets `x/y` regardless of env.
- Outside a git repo with no env / no default_repo, `bb pr list` errors with "no repository specified — use --repo or `bb repo set-default`" (exit code 2).

## Open questions

- Whether to support `BB_CONFIG_FILE` to point at an explicit YAML, or only `BB_CONFIG_DIR`. **Lean: DIR only for MVP.**
- Whether to introduce a `GitRunner` trait now so resolution tests can mock git, or shell out and accept reading the real `.git/config` in unit tests. **Lean: introduce the trait** — same effort, much cleaner test boundary.
