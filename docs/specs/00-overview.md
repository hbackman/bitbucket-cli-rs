# 00 — Overview

**Status:** Draft
**Depends on:** —

## Goal

Build `bb`, a Bitbucket Cloud CLI that mirrors GitHub's `gh` UX. After installing it, an engineer should be able to authenticate via browser, list/view/create/checkout/merge pull requests, view/clone/create repos, and call any REST endpoint via an escape hatch — all from a single static binary.

Success looks like: someone who uses `gh` daily can be productive in `bb` within 5 minutes, because the flags, output formats, and mental model match.

Secondary goal (post-0.1): expose the same Bitbucket surface as an MCP server (`bb mcp serve`), so AI agents using Claude Desktop / Claude Code / Cursor get OAuth-backed Bitbucket access with no extra token setup. See [`09-mcp-server.md`](09-mcp-server.md).

## Non-goals (MVP / 0.1)

- **Bitbucket Server / Data Center** (self-hosted). Different API, doubles scope. Design so a future host abstraction is possible, but don't ship support.
- **Bitbucket Pipelines** surface (`bb pipeline …`). Post-MVP.
- **Issues** surface (`bb issue …`). Post-MVP; few teams use Bitbucket Issues.
- **MCP server** (`bb mcp …`). In scope overall but scheduled for 0.2 — see [`09-mcp-server.md`](09-mcp-server.md). Keep the API/auth layer MCP-friendly (typed structs, no `IoStreams` leakage into business logic) but don't implement the server in the 0.1 push.
- **Extensions system** (`gh extension install`).
- **Aliases** (`gh alias set`).
- Anything GitHub-specific: Codespaces, Copilot, Projects, Actions, Gists, Discussions, Releases.

## Binary name

**Decision: `bb`.**

Conflicts to be aware of:
- Babashka uses `bb`. Document that users with Babashka installed can alias one of them.
- No POSIX shell builtin collides.

If we hit complaints in practice we can ship `bbcli` as a secondary binary name later.

## Target users

Engineers who use Bitbucket Cloud daily and are familiar with (or curious about) `gh`'s mental model. Both interactive use ("did Sam approve that PR?") and scripting ("for each open PR I'm reviewer on, print title") are first-class.

## UX principles

1. **Mirror `gh` whenever the analog is clean.** Same flag names, same default output shape. The cost of inventing new conventions is high; the value is near zero.
2. **TTY-aware output.** Colorized tables when stdout is a TTY; tab-separated plain text when piped. No code in any command should touch `std::io::stdout` directly — go through `IoStreams`.
3. **Repo autodetect + `--repo` override on every repo-scoped command.** Read `git remote get-url origin` and parse; `--repo workspace/slug` always wins.
4. **Every prompt has a flag equivalent.** Interactive UX is allowed, but scripted invocations must work without prompts.
5. **`--json field1,field2` on every list/view.** Plus `--jq EXPR` for transformation. Together these make output composable.
6. **`bb api PATH` on day one.** Whenever someone hits a missing command, the escape hatch saves them.
7. **Stable exit codes.** See [`05-output.md`](05-output.md).
8. **Quiet on success.** `bb` only prints what was asked for; status messages go to stderr.

## Glossary

| Bitbucket term | Meaning |
| --- | --- |
| Workspace | The "owner" of repos. Equivalent to a GitHub org or user namespace. |
| Repo slug | URL-safe lowercase name of a repository. |
| Decline | Close a PR without merging. We expose this as `bb pr close`. |
| Build status | The "checks" concept attached to commits/PRs. |
| App password | Deprecated user-scoped credential (final shutoff 2026-06-09). We do **not** support creating these. |
| API token | The modern replacement for app passwords; user-created, scope-limited. |

## Decision log

| # | Decision | Choice | Rationale |
| --- | --- | --- | --- |
| D1 | Language | Rust 1.75+ | Modern, memory-safe, static binary; async ecosystem aligns with the post-0.1 MCP server. Pivoted from Go after initial draft — gh is the UX reference, not an implementation template. |
| D2 | CLI framework | `clap` v4 (derive) | De-facto standard; subcommand groups, env-var fallbacks, derive macros keep boilerplate low |
| D3 | Bitbucket Server | Out of MVP | Different API; doubles scope |
| D4 | OAuth consumer | Public, baked into binary | Matches gh's pattern |
| D5 | OAuth override | Support `--client-id` / `--client-secret` flags + `BB_OAUTH_CLIENT_ID` / `BB_OAUTH_CLIENT_SECRET` env | For regulated environments |
| D6 | Multi-account | Day-one | Work + personal split is common |
| D7 | Output: `--json` | Day-one | Composability |
| D8 | Output: `--jq` | Day-one (via `jaq` — pure-Rust jq) | Composability; pure-Rust keeps the static binary clean |
| D9 | Output: `--template` | Post-MVP | Less critical given `--json` + `--jq` |
| D10 | Token storage | OS keyring with plaintext fallback | Match gh |
| D11 | Git integration | Shell out to `git` | Avoid libgit2 / git2 native deps |
| D12 | Binary name | `bb` | Short; document Babashka note |
| D13 | License | MIT | Permissive; matches gh |
| D14 | Crate layout | Single binary crate `bb` | Workspace split deferred until the MCP server (spec 09) needs a reusable API crate |
| D15 | Async runtime | `tokio` (multi-thread) | Pairs with `reqwest`; required by the eventual MCP server |
| D16 | MCP server | In scope, post-0.1 | Killer feature vs existing Bitbucket MCP: OAuth-backed, zero token setup. Architecture already supports it; spec is [`09-mcp-server.md`](09-mcp-server.md). Scheduled after 0.1 ships so the CLI isn't blocked. |
| D17 | MCP mutation gating | Off by default | Mutation tools (comment, review, create-pr) hidden unless `--allow-mutations`; merge/decline hidden unless `--allow-merges` too. Agents shouldn't merge without explicit opt-in. |

## Open questions

- Default OAuth scope set (proposed in [`02-authentication.md`](02-authentication.md), needs sign-off).
- Whether to ship a `--profile NAME` flag for multi-account or rely solely on `bb auth switch` (currently leaning on switch; revisit if friction emerges).

## File paths used throughout these specs

- Repo root: `/Users/hbackman/workspace2/bitbucket-cli`
- Crate name: `bb` (binary)
- Config dir: `${XDG_CONFIG_HOME:-$HOME/.config}/bb`
- Cache dir: `${XDG_CACHE_HOME:-$HOME/.cache}/bb`

## Spec map

See [`README.md`](README.md).
