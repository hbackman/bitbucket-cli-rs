# Language Choice — Rust vs Go vs TypeScript

## Summary recommendation

**Go.** Specifically because:
1. We are intentionally building an analog of `gh`, which is a Go project. We can read its source as a reference implementation for *every* hard problem we'll hit (OAuth flow, git credential helper, repo resolution, table rendering, JSON output, factory + IOStreams pattern). That alone is a multi-week head start.
2. Single static binary, easy cross-compile (`GOOS`/`GOARCH`) — meets the "nice binary" requirement.
3. The Cobra/Viper ecosystem is mature and unsexy; that's what we want here.
4. `go-keyring`, `go-git`, and Atlassian-community Bitbucket Go clients exist and are decent.
5. Compile times stay sub-15s — important for a CLI where iteration speed matters.

If we had no reference codebase to copy from, I'd say Rust is also a fine choice. But "copy gh's playbook" is worth more than "slightly smaller binary."

## Comparison

| Concern | Go | Rust | TypeScript (Node) |
| --- | --- | --- | --- |
| Single binary? | ✅ static, ~10–15 MB | ✅ static (musl), ~2–5 MB | ❌ needs Node or `pkg`/`bun`-bundle (~50 MB) |
| Startup time | ~10–50 ms | ~1–10 ms | ~100–300 ms (Node) |
| Build speed | clean 5–15s, incremental fast | clean 1–3 min, incremental 10–30s | instant (interpreted) |
| Auth/HTTP libs | `net/http`, `oauth2`, mature | `reqwest`, `oauth2`, `tokio` — async required | `fetch`, `axios`, `simple-oauth2` |
| OS keyring | `zalando/go-keyring` | `keyring` crate | `keytar` (native deps, painful to ship) |
| CLI framework | Cobra (boring, works) | clap (excellent ergonomics) | oclif / commander / yargs |
| Cross-compile | trivial: `GOOS=darwin GOARCH=arm64 go build` | works but slower; sometimes needs zig/cross | n/a — distribute source |
| `gh` source as a reference | direct copy possible | requires translation | requires translation |
| Hiring/contribution friction | low — Go is widely known | medium — Rust lifetime tax for newcomers | low |

## On the Rust pitch

Rust's advantages — tiny binary, instant startup, clap's UX, type safety — are real but not load-bearing for an internal-ish tool. The single concrete thing we'd give up is the ability to read `cli/cli` and `s/Cobra/clap/g` our way through implementation. Build times also become a UX issue when iterating on a 30+ command surface.

Reasonable Rust path if we go that way:
- `clap` (derive macros) for the command tree
- `reqwest` + `tokio` for HTTP
- `oauth2` crate for the auth flow
- `keyring` crate for token storage
- `directories` for config paths
- `git2` (libgit2) for git interactions, or shell out to `git`

## On TypeScript

Workable but the deployment story is bad for a tool we want users to `brew install`. `bun compile` produces a reasonable single binary in 2026 but adds operational complexity and we'd still be writing async-everywhere code without the perf payoff Rust gives. Skip unless the user actually cares about TS specifically.

## Concrete proposal

- Language: **Go 1.22+**
- CLI framework: **Cobra** (with a thin custom layer if needed)
- HTTP: stdlib `net/http` + a small typed Bitbucket client we build
- OAuth: `golang.org/x/oauth2` (well-tested, handles refresh)
- Keyring: `github.com/zalando/go-keyring`
- Git: shell out to `git` for repo ops; use `git config` to read remotes (matches gh)
- Config: YAML at `~/.config/bb/`, parsed with `gopkg.in/yaml.v3`
- Layout: copy gh — `cmd/bb`, `pkg/cmd/<group>/<verb>/`, `pkg/cmdutil/factory.go`, `pkg/iostreams`, `internal/config`, `internal/auth`, `api/`
- Testing: stdlib `testing` + `httptest.NewServer` for API tests
- Distribution: GoReleaser → Homebrew tap + GitHub Releases binaries

This will feel familiar to anyone who's read the gh source, which is the point.
