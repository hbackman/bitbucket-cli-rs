# `bb` Specs

Specifications for `bb`, a Bitbucket Cloud CLI that mirrors GitHub's `gh` UX.

Each spec is **self-contained and imperative**: a fresh session given "Read `docs/specs/NN-name.md` and follow the instructions" should have enough context to implement that slice without reading the others (except where a `Depends on:` line says otherwise).

| # | Spec | What it covers |
| --- | --- | --- |
| 00 | [Overview](00-overview.md) | Goals, non-goals, decisions, glossary, UX principles |
| 01 | [Architecture](01-architecture.md) | Crate layout, clap + Context + IoStreams skeleton, deps, testing |
| 02 | [Authentication](02-authentication.md) | OAuth loopback flow, API tokens, storage, multi-account, `setup-git` |
| 03 | [Configuration](03-configuration.md) | Config files, env vars, precedence, repo resolution |
| 04 | [API client](04-api-client.md) | HTTP, pagination, retry, errors, `bb api` |
| 05 | [Output](05-output.md) | TTY detection, tables, `--json`, `--jq`, exit codes |
| 06 | [PR commands](06-commands-pr.md) | Every `bb pr` subcommand |
| 07 | [Repo & misc commands](07-commands-repo.md) | `bb repo …`, `bb browse`, `bb version`, `bb completion` |
| 08 | [Distribution](08-distribution.md) | Build, release, Homebrew, versioning |
| 09 | [MCP server](09-mcp-server.md) | `bb mcp serve` — expose Bitbucket as MCP tools, reusing OAuth tokens. Post-0.1. |

## Implementation order

Roughly:

| Order | Spec | Status |
| --- | --- | --- |
| 1 | **01-architecture** — scaffold the binary, dependencies, command tree skeleton, IoStreams, app context, tests pass. | ✅ Done |
| 2 | **03-configuration** — config files, env vars, repo resolution. | ✅ Done |
| 3 | **02-authentication** — OAuth flow, token storage, `bb auth …` commands. | ✅ Done |
| 4 | **04-api-client** — typed REST client, `bb api`. | ✅ Done |
| 5 | **05-output** — `--json`, `--jq`, tables, TTY detection. | ⏭ Next |
| 6 | **07-commands-repo** — `bb repo view/list/clone/create/fork`, `bb browse`, `bb version`. | pending |
| 7 | **06-commands-pr** — PR commands; the bulk of the surface. | pending |
| 8 | **08-distribution** — `cargo-dist`, Homebrew tap, releases. **0.1 ships here.** | pending |
| 9 | **09-mcp-server** — MCP server. Starts after 0.1 lands; targets 0.2. | pending |

Specs 02 and 03 can technically be done in parallel; everything else builds on those plus 01.

### Next session

Paste this into a fresh session to pick up the next slice:

```
Read docs/specs/00-overview.md, docs/specs/01-architecture.md, docs/specs/02-authentication.md, docs/specs/03-configuration.md, docs/specs/04-api-client.md, and docs/specs/05-output.md, then follow 05's instructions.
```

After the slice lands, mark its row ✅ Done and update the "⏭ Next" marker to the row below.

## Background

See [`notes/`](../../notes/) for the research that informed these specs:
- `notes/github-cli.md` — `gh`'s architecture and command surface.
- `notes/bitbucket-auth.md` — Bitbucket OAuth capabilities and constraints.
- `notes/bitbucket-api.md` — Bitbucket REST API basics.
- `notes/language-choice.md` — Language considerations (the initial draft argued for Go; the project pivoted to Rust — see D1 in `00-overview.md`).
- `notes/mvp-scope.md` — MVP scope.

## Spec status

All specs are **Draft**. Treat any "Decision" line as the working answer; treat "Open question" lines as still-needs-resolving before the relevant slice ships.
