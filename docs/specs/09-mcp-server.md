# 09 — MCP Server

**Status:** Draft (Rust; scheduled post-0.1)
**Depends on:** [`01-architecture.md`](01-architecture.md), [`02-authentication.md`](02-authentication.md), [`04-api-client.md`](04-api-client.md)
**Slice goal:** `bb mcp serve` runs an MCP server over stdio that exposes a curated set of read-heavy Bitbucket tools, reusing the OAuth tokens stored by `bb auth login`. `bb mcp install --client claude-desktop` (and friends) registers the server with common clients. The agent gets full Bitbucket access with zero token setup beyond `bb auth login`.

## Why this exists in `bb`

The Bitbucket MCP server already on the market requires you to mint and paste an API token (or worse, an app password) into your client config. We already do an OAuth browser flow for the CLI — once the user runs `bb auth login`, the credentials are in their keyring with auto-refresh. Putting an MCP server in the same binary means there's nothing extra to set up: install `bb`, log in, run `bb mcp install --client claude-code`, done.

Architecturally this is cheap. The MCP server is a thin frontend over `src/auth` + `src/api`, both of which the CLI builds anyway. No new authentication, no new HTTP plumbing — just a different transport and a different argv parser.

## Non-goals

- **Mirroring the entire CLI surface.** An agent doesn't need `bb auth login` or `bb repo clone`; the user does. MCP tools are a curated subset.
- **A separate `bb-mcp` binary.** Single binary keeps install simple. The MCP server lives behind `bb mcp serve`.
- **A hosted / remote MCP server.** Stdio first. SSE/HTTP transport possible later but out of scope for the first MCP slice.
- **Plugin system for tools.** We define a fixed set; agents don't discover ad-hoc tools.

## Subcommand surface

```
bb mcp serve [flags]            # the actual server (long-running stdio process)
bb mcp install [flags]          # write client config to register the server
bb mcp tools                    # list available MCP tools (introspection)
```

### `bb mcp serve`

```
Flags:
      --transport <KIND>         "stdio" (default) or "sse" (post-0.2)
      --port <PORT>              Port for sse transport
      --hostname <HOST>          Bitbucket host (default from config)
      --user <USER>              Pin to a specific stored account
      --allow-mutations          Enable write tools (comment, review, create-pr, update-pr)
      --allow-merges             Enable merge / decline (requires --allow-mutations)
      --tools <NAMES>            Allowlist tool names (comma-separated)
      --no-tools <NAMES>         Blocklist tool names (comma-separated)
      --log-file <PATH>          Write logs here (stdio transport must keep stdout clean)
      --log-level <LEVEL>        trace|debug|info|warn|error (default "info")
      --read-only                Alias for omitting --allow-mutations
```

Capability tiers:

| Tier | Flag | Tools enabled |
| --- | --- | --- |
| Read (default) | — | View / list / search / diff tools |
| Mutate | `--allow-mutations` | Above + comment, review, create/update PR |
| Merge | `--allow-mutations --allow-merges` | Above + merge, decline |

Refusing to register higher-tier tools by default is intentional: the agent should not be able to merge or decline a PR unless the user explicitly opts in.

### `bb mcp install`

```
Flags:
      --client <NAME>            "claude-desktop" | "claude-code" | "cursor" | "vscode-copilot"
      --scope <SCOPE>            "user" (default) or "project" (writes to ./.claude/settings.json or equivalent)
      --print                    Print the JSON snippet instead of writing it
      --allow-mutations          Bake into the generated config
      --allow-merges             Bake into the generated config
      --name <NAME>              Server name (default "bb")
      --force                    Overwrite an existing entry without prompting
```

Examples of what it writes — Claude Desktop (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "bb": {
      "command": "/usr/local/bin/bb",
      "args": ["mcp", "serve"]
    }
  }
}
```

Claude Code (project-scope at `./.claude/settings.json`):

```json
{
  "mcp": {
    "servers": {
      "bb": {
        "command": "bb",
        "args": ["mcp", "serve"]
      }
    }
  }
}
```

If the config file already has a `bb` entry, prompt before overwriting (unless `--force`). `--print` skips the file and writes the snippet to stdout.

### `bb mcp tools`

```
$ bb mcp tools
NAME                CATEGORY    MUTATION  DESCRIPTION
bb_pr_list          read        no        List pull requests in a Bitbucket repo
bb_pr_view          read        no        View a single pull request
bb_pr_diff          read        no        Get the unified diff for a pull request
...
```

`--json` for structured output.

## MCP library

**Recommendation:** use the official Rust MCP SDK `rmcp` (`github.com/modelcontextprotocol/rust-sdk`) once it stabilizes. As of writing it's actively maintained and provides `Server`, `Transport`, and `ServerHandler` traits with stdio support.

Alternative: `mcp_rust_sdk` (community). Verify maturity at implementation time.

The trait wrapper below is intentionally library-agnostic so swapping is a localized change.

## Tool surface

Tools are namespaced `bb_<resource>_<verb>`.

### Read tools (always enabled)

| Tool | Maps to | Description |
| --- | --- | --- |
| `bb_pr_list` | `GET /repositories/{ws}/{repo}/pullrequests` | List PRs with filters |
| `bb_pr_view` | `GET .../pullrequests/{id}` | Get PR details (title, body, state, branches, reviewers) |
| `bb_pr_diff` | `GET .../pullrequests/{id}/diff` | Unified diff |
| `bb_pr_commits` | `GET .../pullrequests/{id}/commits` | List PR commits |
| `bb_pr_comments` | `GET .../pullrequests/{id}/comments` | List comments (general + inline) |
| `bb_pr_checks` | `GET .../pullrequests/{id}/statuses` | Build statuses |
| `bb_pr_activity` | `GET .../pullrequests/{id}/activity` | Timeline (approvals, changes-requested, comments) |
| `bb_repo_view` | `GET /repositories/{ws}/{repo}` | Repo metadata + README |
| `bb_repo_list` | `GET /repositories/{ws}` or `/user/permissions/repositories` | List accessible repos |
| `bb_repo_branches` | `GET /repositories/{ws}/{repo}/refs/branches` | List branches |
| `bb_repo_file` | `GET /repositories/{ws}/{repo}/src/{ref}/{path}` | Read a file at a ref |
| `bb_search_prs` | `GET /repositories/{ws}/{repo}/pullrequests?q=...` | BBQL search |
| `bb_workspaces_list` | `GET /workspaces` | List workspaces the user can access |
| `bb_user_me` | `GET /user` | Current user info |

### Mutate tools (require `--allow-mutations`)

| Tool | Maps to | Description |
| --- | --- | --- |
| `bb_pr_create` | `POST .../pullrequests` | Open a PR |
| `bb_pr_update` | `PUT .../pullrequests/{id}` | Edit title/body/reviewers |
| `bb_pr_comment` | `POST .../pullrequests/{id}/comments` | Add a comment |
| `bb_pr_review` | various | Approve / unapprove / request-changes / un-request-changes |
| `bb_pr_ready` | `PUT .../pullrequests/{id}` (draft=false) | Mark draft as ready |

### Merge tools (require `--allow-merges`)

| Tool | Maps to | Description |
| --- | --- | --- |
| `bb_pr_merge` | `POST .../pullrequests/{id}/merge` | Merge a PR |
| `bb_pr_decline` | `POST .../pullrequests/{id}/decline` | Decline a PR |

### Deliberately not exposed

- `bb_auth_*` — credentials lifecycle belongs in the user's terminal.
- `bb_repo_clone` / `bb_repo_create` / `bb_repo_fork` — agents shouldn't manage repos at this layer; let them ask the user.
- `bb_api` (generic passthrough) — too broad; agents should use the typed tools.
- `bb_browse` — agents don't have a browser.

If demand emerges, add them post-0.2 and document the risk.

## Tool schema principles

1. **Resource identifiers are plain strings**: `repo` parameter is `"workspace/slug"`, never separate `workspace` + `slug` fields. Easier for the model.
2. **Integers are integers**: PR IDs use `"type": "integer"`, not strings.
3. **Optional repo via current context**: tools accept `repo: string` (optional). If omitted and the server was started inside a git clone, we resolve from `git remote get-url origin` (same as the CLI). Otherwise return an error asking for `repo`.
4. **Limits and pagination**: list tools accept `limit` (default 30, max 100) and `cursor` (opaque page token). Return the next cursor when there's more.
5. **Output is JSON, not prose**: every tool returns structured data. The agent does the prose. Errors include a `hint` field with the human-actionable fix.
6. **Stable field names that match Bitbucket's vocabulary**: `source_branch`, `destination_branch`, `state` (OPEN/MERGED/DECLINED), `decline` (not "close"). Avoid GitHub-isms in the MCP layer — the LLM can translate.

## Example: `bb_pr_view`

Schema (constructed via `serde_json::json!` and passed to the SDK at registration):
```json
{
  "name": "bb_pr_view",
  "description": "View a Bitbucket pull request: title, body, state, source/destination branches, reviewers, build statuses, and comment count.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "repo": {
        "type": "string",
        "description": "workspace/slug (e.g. acme/api). Optional if the server is running inside a git clone of the target repo."
      },
      "id": {
        "type": "integer",
        "description": "Pull request id"
      },
      "include_body": {
        "type": "boolean",
        "default": true
      }
    },
    "required": ["id"]
  }
}
```

Output (returned as MCP `content: [{ type: "text", text: <JSON> }]`):
```json
{
  "id": 42,
  "title": "Fix login flow",
  "body": "Fixes the broken redirect...",
  "state": "OPEN",
  "draft": false,
  "author": { "username": "hbackman", "display_name": "Hampus" },
  "source_branch": "feature/login",
  "destination_branch": "main",
  "reviewers": [
    { "username": "alice", "approved": true, "requested_changes": false },
    { "username": "bob", "approved": false, "requested_changes": true }
  ],
  "checks": {
    "summary": "1 success, 1 failed",
    "items": [ { "name": "ci/test", "state": "SUCCESSFUL", "url": "..." } ]
  },
  "url": "https://bitbucket.org/acme/api/pull-requests/42",
  "comment_count": 7,
  "created_on": "2026-05-14T08:21:00Z",
  "updated_on": "2026-05-14T10:11:00Z"
}
```

Each tool re-uses the JSON shapes defined in `src/api/types.rs` (spec 04) — pure data, identical to what the CLI sees.

## Logging on stdio

Critical: stdio transport uses stdin/stdout for the MCP protocol. **Logs cannot go to stdout.** Default routing:

- `--log-file` → write there.
- Otherwise → stderr.
- Never stdout.

Use `tracing` + `tracing-subscriber` with a JSON formatter. Default level `info`. Sensitive values (auth headers) are never logged.

```rust
fn install_logging(opts: &ServeOpts) -> Result<()> {
    let writer = match &opts.log_file {
        Some(path) => BoxMakeWriter::new(non_blocking_file(path)?),
        None       => BoxMakeWriter::new(std::io::stderr),
    };
    tracing_subscriber::fmt()
        .json()
        .with_writer(writer)
        .with_max_level(parse_level(&opts.log_level)?)
        .init();
    Ok(())
}
```

## Tool implementation pattern

```rust
// src/mcp/tools/pr_view.rs
use async_trait::async_trait;

pub struct PrView;

#[async_trait]
impl Tool for PrView {
    fn name(&self) -> &str        { "bb_pr_view" }
    fn description(&self) -> &str { "..." }
    fn capability(&self) -> Capability { Capability::Read }
    fn input_schema(&self) -> serde_json::Value { pr_view_schema() }

    async fn run(&self, deps: &Deps, args: serde_json::Value) -> ToolResult {
        #[derive(serde::Deserialize)]
        struct In { #[serde(default)] repo: Option<String>, id: u32 }

        let input: In = match serde_json::from_value(args) {
            Ok(v) => v,
            Err(e) => return ToolResult::error(format!("invalid arguments: {e}"), "Check the input schema."),
        };
        let repo = match resolve_repo(deps, input.repo.as_deref()).await {
            Ok(r) => r,
            Err(e) => return ToolResult::error(e.to_string(), "Pass `repo` as workspace/slug."),
        };
        match deps.api.pull_requests().get(&repo, input.id).await {
            Ok(pr) => ToolResult::json(present_pr(&pr)),
            Err(e) => wrap_api_error(e),
        }
    }
}
```

`Registry` in `src/mcp/registry.rs` collects all tools and filters by capability flags before exposing them via the SDK's `tools/list`.

## File layout

```
src/cli/mcp/
├── mod.rs                  # `bb mcp <verb>` clap subcommand tree
├── serve.rs                # `bb mcp serve` — argv parsing, wires registry + transport
├── install.rs              # `bb mcp install --client X`
├── install_targets.rs      # config paths per client
└── tools_cmd.rs            # `bb mcp tools` (introspection table)

src/mcp/
├── mod.rs                  # public surface: Server, Capability, Tool trait
├── registry.rs             # capability filtering, --tools / --no-tools, registration with SDK
├── repo_context.rs         # resolve repo from cwd / git remote
├── deps.rs                 # Deps struct: ApiClient + AuthSource + logger
└── tools/
    ├── mod.rs              # Tool trait, ToolResult, Capability
    ├── helpers.rs          # resolve_repo, json/error helpers, wrap_api_error
    ├── pr_list.rs
    ├── pr_view.rs
    ├── pr_diff.rs
    ├── pr_commits.rs
    ├── pr_comments.rs
    ├── pr_checks.rs
    ├── pr_activity.rs
    ├── pr_create.rs
    ├── pr_update.rs
    ├── pr_comment.rs
    ├── pr_review.rs
    ├── pr_ready.rs
    ├── pr_merge.rs
    ├── pr_decline.rs
    ├── repo_view.rs
    ├── repo_list.rs
    ├── repo_branches.rs
    ├── repo_file.rs
    ├── search_prs.rs
    ├── workspaces_list.rs
    └── user_me.rs
```

## Required dependencies (deferred from spec 01)

```toml
rmcp                 = "0.x"                 # exact version TBD at implementation time
tracing              = "0.1"
tracing-subscriber   = { version = "0.3", features = ["json", "env-filter"] }
async-trait          = "0.1"
```

(If `rmcp` isn't ready, swap to `mcp_rust_sdk` or whichever crate has stabilized.)

## Tests

For each tool: unit test that constructs `Deps` with a `wiremock` server, calls `run` with sample args, asserts the returned JSON.

Server-level integration test:
1. Start the server in-process by spinning up two duplex pipes for stdin/stdout.
2. Send `{"jsonrpc":"2.0","id":1,"method":"tools/list"}`.
3. Assert the expected tool set (filtered by capability) appears.
4. Send `{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"bb_pr_view","arguments":{...}}}`.
5. Assert the response shape.

Capability tests:
- Default → mutation tools absent from `tools/list`.
- `--allow-mutations` → mutation tools present, merge tools absent.
- `--allow-mutations --allow-merges` → all tools present.

Install tests:
- `bb mcp install --client claude-desktop --print` writes the documented JSON to stdout.
- Writing to an existing config preserves other entries (round-trip safe via `serde_json::Value` and atomic rename).

## Concrete deliverables

1. `src/cli/mcp/` and `src/mcp/` modules per the layout above, fully implemented.
2. All tools in `src/mcp/tools/`.
3. `src/cli/mcp/install.rs` supporting at minimum `claude-desktop`, `claude-code`, `cursor`.
4. `src/cli/mcp/tools_cmd.rs` for `bb mcp tools`.
5. Tests as listed.
6. Register `bb mcp` in `src/cli/mod.rs` under the "Additional commands" group.
7. README update: a short section explaining how to use `bb` as an MCP server with Claude Code / Desktop.

## Acceptance criteria

- `bb mcp serve` over stdio:
  - Responds to `tools/list` with read-tier tools only (default).
  - With `--allow-mutations`, registers the mutate tier.
  - With `--allow-mutations --allow-merges`, registers all tools.
  - Logs go to stderr (or `--log-file`), never stdout.
- `bb mcp install --client claude-desktop` writes a valid `claude_desktop_config.json` entry that, after restart, exposes `bb_*` tools to the agent.
- An agent invoking `bb_pr_view` with `{"id": 42}` from a git clone of `acme/api` returns the PR's JSON.
- Mutation tools refuse to run when not registered (the schema isn't even exposed — `tools/list` omits them).
- Auth refresh happens transparently inside the server (same flow as CLI commands).
- A misconfigured token returns a tool error containing `"hint": "Run 'bb auth login' to re-authenticate."`

## Open questions

- Which MCP Rust library to use — verify at implementation time. Candidates: `rmcp` (official), `mcp_rust_sdk` (community).
- Whether to expose `bb_api` as a generic passthrough behind `--allow-passthrough`. **Lean: no** in the first MCP slice; LLM-driven freeform API calls are an audit nightmare.
- Whether `bb mcp install --scope project` for Claude Code should also create `.claude/settings.local.json` instead of `.claude/settings.json`. **Lean: settings.json** since the user probably wants this checked in.
- Whether to add a `bb_pr_inline_comment` tool (per-file/per-line). **Lean: yes** if the API surface is ergonomic enough; defer otherwise.
- Rate-limit / audit logging — should mutations be logged to a local audit file by default? **Lean: yes**, write to `${XDG_STATE_HOME}/bb/mcp-audit.log` whenever a mutating tool runs.
- Should the server refuse to start when `BB_TOKEN` is the only credential (no stored OAuth)? Tokens via env have no refresh, so the server may die after 2 hours. **Lean: warn but allow.**
