# 04 — API Client

**Status:** Draft (Rust)
**Depends on:** [`01-architecture.md`](01-architecture.md), [`02-authentication.md`](02-authentication.md), [`03-configuration.md`](03-configuration.md)
**Slice goal:** A typed Bitbucket REST client used by every command, plus the `bb api` escape hatch. Handles auth header injection, automatic token refresh on 401, pagination as an async stream, rate-limit-aware retry on 429, and structured error responses.

## Scope

- Cover the endpoints listed in [`bitbucket-api.md`](../../notes/bitbucket-api.md): repositories, pull requests, branches, current user, statuses, comments, activity, diffs.
- Issues, pipelines, webhooks: leave the typed wrappers unwritten in MVP — they can be reached via `bb api`.
- GraphQL: not applicable (Bitbucket Cloud has no GraphQL).

## Layers

1. **Transport** (`src/api/transport.rs`): low-level HTTP plumbing on `reqwest::Client`. Auth injection, retries, debug logging, error parsing.
2. **Client** (`src/api/client.rs`): a typed `Client` struct with grouped resource methods. Each method returns deserialized structs, not `serde_json::Value`.
3. **`bb api` escape hatch** (`src/cli/api.rs`): passes a user-specified path through the transport, with helpful conveniences.

## Base configuration

- Base URL: `https://api.bitbucket.org/2.0`
- User agent: `bb/<version> (+https://github.com/hbackman/bitbucket-cli)` (constructed from `BuildInfo`)
- Timeout: 30s per request (configurable on the `Client`).
- Default `Accept: application/json`.
- Default `Content-Type: application/json` for write methods.

The shared `reqwest::Client` lives on `Context::http` and is constructed once with the user agent, timeout, and rustls TLS backend.

## Transport (`src/api/transport.rs`)

```rust
pub struct Transport {
    pub http: reqwest::Client,
    pub auth: Arc<AuthSource>,          // from src/auth
    pub host: String,                   // "bitbucket.org"
    pub user_agent: String,
    pub debug: DebugMode,
}

#[derive(Debug, Clone, Copy)]
pub enum DebugMode { Off, On, Verbose }  // BB_DEBUG=0 | 1 | api:verbose

impl Transport {
    pub async fn send(&self, mut req: reqwest::Request) -> Result<reqwest::Response, ApiError> {
        // 1. Inject Authorization + User-Agent (idempotent).
        // 2. Send.
        // 3. On 429: honor Retry-After (≤ 60s, max 2 retries) → RateLimit error otherwise.
        // 4. On 401: ask self.auth.refresh_now(...), retry once → AuthError otherwise.
        // 5. On status >= 400: parse the Bitbucket error envelope, return ApiError::Response.
        // 6. Otherwise return the response.
    }
}
```

Because `reqwest::Request` isn't cheaply cloneable when it has a streaming body, the retry path either:
- For idempotent methods (GET/HEAD/OPTIONS): replays from the original URL/headers (no body to re-stream).
- For mutating methods: refuses to retry after the body was sent and surfaces the original error with a `hint` pointing to `bb api --retry`.

In practice the only retry that runs on POST/PUT is the 401 → refresh → retry path; we capture the bytes up-front using `reqwest::Body::from(Vec<u8>)` for those requests so a single replay is safe.

Debug logging (`DebugMode != Off`) dumps:
- Request line, headers (redact `Authorization` unless `DebugMode::Verbose`), and a 4KB-truncated body, **before** the send.
- Status, headers, and 4KB-truncated body **after** the send.

Writes go to stderr (or to `BB_DEBUG_FILE` when set — useful for stdio-only contexts like MCP).

### Error types

```rust
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("network: {0}")]
    Network(#[from] reqwest::Error),

    #[error("authentication failed")]
    Auth { hint: String },

    #[error("rate limited (retry after {retry_after_secs}s)")]
    RateLimit { retry_after_secs: u64 },

    #[error("{status}: {message}")]
    Response {
        status: reqwest::StatusCode,
        method: reqwest::Method,
        url: String,
        message: String,                  // parsed from envelope, fallback to status line
        errors: Vec<BitbucketError>,
        raw: bytes::Bytes,                // first 4KB
    },
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct BitbucketError {
    pub message: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

impl From<ApiError> for CliError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::Auth { hint }      => CliError::Auth(hint),
            ApiError::RateLimit { .. }   => CliError::Other(anyhow::Error::from(e)), // exit 5 via NewType
            ApiError::Response { .. }    => CliError::Other(anyhow::Error::from(e)),
            ApiError::Network(_)         => CliError::Other(anyhow::Error::from(e)),
        }
    }
}
```

Bitbucket error envelope shape:
```json
{ "type": "error", "error": { "message": "...", "detail": "..." } }
```

(`CliError` gets a `RateLimit` variant in spec 05 — wired then.)

## Client (`src/api/client.rs`)

```rust
pub struct Client {
    transport: Arc<Transport>,
    base: url::Url,
}

impl Client {
    pub fn new(transport: Arc<Transport>) -> Self { /* ... */ }

    pub fn user(&self)         -> UserService          { /* ... */ }
    pub fn repositories(&self) -> RepositoryService    { /* ... */ }
    pub fn pull_requests(&self) -> PullRequestService  { /* ... */ }
    pub fn workspaces(&self)   -> WorkspaceService     { /* ... */ }
    pub fn branches(&self)     -> BranchService        { /* ... */ }
}
```

Each resource service lives in its own file: `src/api/user.rs`, `src/api/repository.rs`, `src/api/pull_request.rs`, etc. Services borrow the transport from `Client` (cheap — `Arc` clone).

### Resource service shape

```rust
// src/api/pull_request.rs
pub struct PullRequestService<'a> { client: &'a Client }

impl<'a> PullRequestService<'a> {
    pub fn list(&self, repo: &BbRepo, opts: ListOpts) -> Paginated<PullRequest>;
    pub async fn get(&self, repo: &BbRepo, id: u32) -> Result<PullRequest, ApiError>;
    pub async fn create(&self, repo: &BbRepo, input: &CreatePr) -> Result<PullRequest, ApiError>;
    pub async fn update(&self, repo: &BbRepo, id: u32, input: &UpdatePr) -> Result<PullRequest, ApiError>;
    pub async fn merge(&self, repo: &BbRepo, id: u32, input: &MergeInput) -> Result<PullRequest, ApiError>;
    pub async fn decline(&self, repo: &BbRepo, id: u32) -> Result<PullRequest, ApiError>;
    pub async fn approve(&self, repo: &BbRepo, id: u32) -> Result<(), ApiError>;
    pub async fn unapprove(&self, repo: &BbRepo, id: u32) -> Result<(), ApiError>;
    pub async fn request_changes(&self, repo: &BbRepo, id: u32) -> Result<(), ApiError>;
    pub async fn unrequest_changes(&self, repo: &BbRepo, id: u32) -> Result<(), ApiError>;
    pub async fn diff(&self, repo: &BbRepo, id: u32) -> Result<reqwest::Response, ApiError>; // streamed body
    pub fn commits(&self, repo: &BbRepo, id: u32) -> Paginated<Commit>;
    pub fn comments(&self, repo: &BbRepo, id: u32) -> Paginated<Comment>;
    pub async fn add_comment(&self, repo: &BbRepo, id: u32, body: &str) -> Result<Comment, ApiError>;
    pub fn statuses(&self, repo: &BbRepo, id: u32) -> Paginated<BuildStatus>;
    pub fn activity(&self, repo: &BbRepo, id: u32) -> Paginated<Activity>;
}
```

Each input/output struct lives in `src/api/types.rs` (or its resource file) with `#[derive(Debug, Clone, Serialize, Deserialize)]`. Time fields use `time::OffsetDateTime` with serde's RFC3339 feature.

`ListOpts` for PRs:
```rust
#[derive(Debug, Default, Clone)]
pub struct ListOpts {
    pub state: Option<PrState>,          // OPEN/MERGED/DECLINED/SUPERSEDED
    pub query: Option<String>,           // BBQL → ?q=
    pub author: Option<String>,
    pub page_len: Option<u32>,
    pub fields: Option<String>,          // sparse-fieldset → ?fields=
}
```

### Pagination as an async stream

```rust
pub struct Paginated<T> { /* internal: client + first URL + buffer + next */ }

impl<T: serde::de::DeserializeOwned + Unpin + Send + 'static> Paginated<T> {
    /// Returns a stream of items. Internally fetches one page at a time.
    pub fn into_stream(self) -> impl futures::Stream<Item = Result<T, ApiError>>;

    /// Collect up to `limit` items, stopping at the page boundary that crosses `limit`.
    pub async fn collect(self, limit: usize) -> Result<Vec<T>, ApiError>;
}
```

Implementation uses `async-stream::try_stream!` (a tiny dep) or rolls a manual `Stream` impl. We don't add `futures-util` as a hard dep; `tokio::pin!` + manual polling is enough.

Bitbucket page envelope:
```json
{ "values": [...], "next": "https://...", "pagelen": 30, "page": 1, "size": 142 }
```

The stream stops when `next` is absent or null.

## `bb api` command

```
bb api <endpoint> [flags]

Flags:
  -X, --method <METHOD>          HTTP method (default GET, inferred from --field/--input)
  -H, --header <KEY:VALUE>       Add request header (repeatable)
  -F, --field <KEY=VALUE>        Typed field: parsed as bool/int/null/string
  -f, --raw-field <KEY=VALUE>    Raw string field
      --input <PATH>             Body file ("-" for stdin)
      --paginate                 Follow `next` links and concatenate `values`
      --slurp                    With --paginate, emit one JSON array of all pages
  -q, --jq <EXPR>                Filter output with jq (via `jaq`)
  -t, --template <EXPR>          Format output with Tera template (post-MVP)
      --hostname <HOST>          Override host
      --include                  Include HTTP response headers in output
      --silent                   Do not print response body
      --cache <DURATION>         Cache successful GET responses (e.g. "10m")
```

Behaviors:

- `<endpoint>` can be a full URL or a path. Paths starting with `/` get prefixed with `https://api.bitbucket.org/2.0` (or whatever host). Paths starting with `2.0/` get the host prefix only.
- `{workspace}`, `{repo}`, `{branch}` placeholders in the path are replaced from the current repo context when present. E.g. `bb api repos/{workspace}/{repo}/pullrequests`.
- Method inference: any `--field` / `--input` / `--raw-field` flips the default to `POST`.
- `--field key=true|false|null` parses booleans/null; numeric strings parse to numbers. `--raw-field` keeps strings verbatim.
- For `GET`, fields go into the query string; otherwise into a JSON body.
- `--paginate` follows the `next` URL on each response. With `--slurp` it accumulates all `values` arrays into one JSON array; without, it prints each page separated by a newline.
- `--cache 10m` writes successful GET responses to `${XDG_CACHE_HOME}/bb/api/<hash>` and replays them within the window.

Examples (these should work after this slice):

```
bb api /user
bb api repos/{workspace}/{repo}/pullrequests --paginate
bb api -X POST repos/{workspace}/{repo}/pullrequests \
  -f title="My PR" \
  -f source.branch.name=feature \
  -f destination.branch.name=main
bb api /user --jq '.username'
```

Durations parsed via `humantime` (`"10m"`, `"1h30s"`).

## Debug mode

`BB_DEBUG=1` → `DebugMode::On` (redact `Authorization`).
`BB_DEBUG=api:verbose` → `DebugMode::Verbose` (no redaction). Don't enable in CI by accident.
`BB_DEBUG_FILE=<path>` → redirect debug logs to a file instead of stderr (useful for MCP stdio).

## Caching (only on `bb api --cache`)

- Key: SHA-256 of method + URL + sorted query + Accept header.
- Value: status, headers, body, written via `serde_json::to_writer` against a small envelope struct.
- Location: `${XDG_CACHE_HOME}/bb/api/`. One file per entry.
- Eviction: lazy, on read. If `mtime < now - cache-duration`, ignore + delete.
- No background eviction in MVP.

## File layout

```
src/api/
├── mod.rs                # re-exports Client, Transport, ApiError, types
├── client.rs
├── transport.rs
├── errors.rs
├── pagination.rs         # Paginated<T> + Stream impl
├── debug.rs
├── cache.rs              # `bb api --cache` only
├── user.rs
├── workspace.rs
├── repository.rs
├── branch.rs
├── pull_request.rs
├── comment.rs
├── status.rs
├── activity.rs
└── types.rs              # shared structs: User, Repo, Branch, etc.

src/cli/api.rs            # `bb api` command (replaces the stub from spec 01)
src/cli/api/
├── fields.rs             # --field / --raw-field parsing
└── pagination.rs         # --paginate / --slurp output handling
```

(The stub `src/cli/api.rs` from spec 01 is promoted to a directory with `mod.rs` when this slice lands.)

## Required dependencies (deferred from spec 01)

Add to `Cargo.toml` when this slice lands:

```toml
async-stream = "0.3"
bytes        = "1"
humantime    = "2"
sha2         = "0.10"
hex          = "0.4"
time         = { version = "0.3", features = ["serde", "macros", "formatting", "parsing"] }
```

`reqwest`, `serde`, `serde_json`, `tokio`, `thiserror`, `anyhow`, `url` are already pinned.

## Tests

`wiremock` is the standard mock server for `reqwest`-based tests; add it to dev-dependencies in this slice. Cover:

- Auth header is injected.
- 401 triggers a refresh + retry; a second 401 returns `ApiError::Auth`.
- 429 with `Retry-After: 1` retries successfully; after 2 attempts returns `ApiError::RateLimit`.
- 4xx parses the Bitbucket error envelope.
- Pagination follows `next` and stops on null.
- `bb api /user` (mocked) prints the expected JSON.
- `bb api --field a=1 --field b=true /thing` sends `{"a":1,"b":true}`.
- `bb api --paginate /list` walks all pages.
- `bb api --jq '.username' /user` filters output via `jaq`.
- Debug mode (`On`) redacts the `Authorization` header; `Verbose` does not.

## Concrete deliverables

1. `src/api/transport.rs`, `errors.rs`, `pagination.rs`, `debug.rs`, `cache.rs`.
2. `src/api/client.rs` with the service-grouping pattern.
3. `src/api/{user,repository,branch,pull_request,comment,status,activity,workspace,types}.rs`. Full implementations of every `PullRequestService` method listed above.
4. `src/cli/api/` directory + helpers.
5. Wire `Context::http` and `Context::api` in spec 01's lazy fields. The `pub struct Client;` placeholder in `src/api/mod.rs` is replaced by the real client.
6. `BB_DEBUG` plumbing.
7. Tests as listed above.

## Acceptance criteria

- `bb api /user` returns the authenticated user as JSON.
- `bb api repos/{workspace}/{repo} --jq '.full_name'` prints the workspace/slug from the current clone.
- `bb api repos/{workspace}/{repo}/pullrequests --paginate --slurp | jq '. | length'` returns total PR count.
- Token refresh on 401 is exercised by an integration test.
- 429 with `Retry-After` is honored.
- Error responses are surfaced clearly (status code + Bitbucket message + path).
- All higher-level command code calls `ctx.api()` and never constructs a `reqwest::Client` directly.

## Open questions

- Should `Paginated<T>` implement `futures::Stream` directly, or expose a method that returns one? **Lean: expose `into_stream()`** — keeps the type concrete and avoids leaking lifetimes.
- Cache key should include the access token's user (so different accounts get separate caches), or not? **Lean: include user** — same URL can return different data for different accounts.
- Should `bb api` infer `{workspace}/{repo}` from the current repo *automatically*, or only when the placeholder tokens appear? **Lean: only when placeholders appear** — keeps behavior explicit.
