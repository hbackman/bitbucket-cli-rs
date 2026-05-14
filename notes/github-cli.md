# GitHub CLI (`gh`) — Research Notes

Source: https://github.com/cli/cli, https://cli.github.com/manual

## What it is

`gh` is GitHub's official command-line tool. It is a **standalone tool** (not a `git` proxy like the old `hub`). Written in **Go** (~99% of the codebase). Distributed as a single static binary via Homebrew / package managers / precompiled releases. Build provenance is attested via Sigstore.

## Top-level command groups

These are the relevant ones for a Bitbucket equivalent (Copilot, Codespaces, Actions, Projects, etc. are GitHub-specific and we should skip):

| Group | Purpose | MVP priority |
| --- | --- | --- |
| `auth` | login/logout/status/token/refresh/switch/setup-git | **must** |
| `repo` | create, clone, fork, view, list, delete, edit, set-default, sync | **must** (subset) |
| `pr` | create, list, view, checkout, merge, close, reopen, diff, review, comment, edit, checks | **must** |
| `issue` | create, list, view, edit, close, comment | nice-to-have (Bitbucket has Issues but few teams use them) |
| `api` | direct REST/GraphQL calls — "escape hatch" | **must** |
| `browse` | open the repo/PR in the browser | easy win |
| `config` | get/set/list config | **must** |
| `alias` | user-defined command aliases | post-MVP |
| `completion` | shell completion scripts | post-MVP |
| `release` | manage releases | Bitbucket has no real equivalent — skip |
| `run` / `workflow` | GitHub Actions | replace with `pipelines` for Bitbucket Pipelines, post-MVP |
| `gist` / `codespace` / `copilot` / `project` / `attestation` | GitHub-only | skip |

Global concerns shared across all commands:
- Auth token resolution (`$GH_TOKEN` / `$GITHUB_TOKEN` env vars override stored token).
- Host selection (gh supports github.com + GHES; we likely only need bitbucket.org for MVP, but design with host pluggability in mind).
- `--repo OWNER/REPO` override; otherwise resolve from current git remote.
- JSON output (`--json field1,field2 --jq ...`) — very useful for scripting; should bake in from day one.

## Subcommand inventory (the bits we care about)

### `gh pr`
`create`, `list`, `view`, `status`, `checkout`, `checks`, `close`, `comment`, `diff`, `edit`, `lock`/`unlock`, `merge`, `ready`, `reopen`, `revert`, `review`, `update-branch`.

For MVP we probably need: **create, list, view, checkout, diff, merge, close, comment, review, checks**.

### `gh repo`
`create`, `list`, `view`, `clone`, `fork`, `sync`, `archive`/`unarchive`, `delete`, `edit`, `rename`, `set-default`, `deploy-key`, `autolink`, `gitignore`, `license`.

For MVP: **create, list, view, clone, fork** (Bitbucket forks differ from GitHub — see `bitbucket-api.md`).

### `gh auth`
`login`, `logout`, `status`, `switch`, `refresh`, `token`, `setup-git`.

`setup-git` is interesting: it configures `git` so that `git push` over HTTPS uses the stored token as a credential helper. We should do the same — `bb auth setup-git`.

## Architecture (worth copying)

From the public repo layout (`cli/cli`):

```
cmd/gh/                # main entrypoint
pkg/cmd/<group>/       # one directory per command group
  pr/
    create/
    list/
    view/
    ...
    pr.go              # parent command, wires subcommands
pkg/cmdutil/           # the Factory pattern (DI container)
api/                   # REST + GraphQL client wrappers
internal/              # config, auth, git, term I/O
```

Key design choices:
- **Cobra** for command tree.
- **Factory pattern** (`cmdutil.Factory`): every command receives an injected factory with lazy-initialized fields for HTTP client, config, git, IO streams, branding. Makes testing dramatically easier.
- **Two-level command tree**: `gh <group> <verb>`. Each `<verb>` lives in its own subpackage with `cmd.go`, `cmd_test.go`, sometimes `http.go` for API calls used only by that verb.
- **IOStreams abstraction**: stdout/stderr/stdin + color detection + pager wiring is centralized. Every command takes an `IOStreams` rather than touching `os.Stdout` directly.
- **Shared package per group** (e.g. `pkg/cmd/pr/shared`) for things like PR finders, formatters that span multiple subcommands.

## Authentication (`gh auth login`)

Two paths:
1. **Web browser OAuth (default).** Opens browser, user authenticates, token is written to system credential store (macOS Keychain, Windows Credential Manager, libsecret on Linux). Falls back to plaintext on `~/.config/gh/hosts.yml` if no keyring.
2. **Token paste / `--with-token` from stdin.** For headless/CI use.

The OAuth flow uses GitHub's **device-authorization-grant**-flavored experience: the CLI shows a one-time code and the user pastes it in the browser. (Optional `--web --clipboard` copies it automatically.)

Token storage in priority order:
1. `GH_TOKEN` env var (highest)
2. `GITHUB_TOKEN` env var
3. System keyring
4. `~/.config/gh/hosts.yml` (plaintext fallback)

`gh auth status` reports which one it's using; `gh auth token` prints the resolved token.

**Implication for `bb`:** Bitbucket does **not** support OAuth device flow — see `bitbucket-auth.md` for what we can do instead (loopback localhost redirect works, which is essentially equivalent UX-wise).

## Output / UX patterns worth stealing

- **Smart TTY detection.** Tables + colors when interactive; tab-separated plain output when piped (`gh pr list | grep …` works naturally).
- **`--json` + `--jq` + `--template`** on every list/view command. Makes the tool composable.
- **Auto-resolves the current repo** from the local git remote so `gh pr list` "just works" in a clone. `--repo OWNER/REPO` overrides.
- **Interactive prompts** for `create` commands when fields are missing, but every prompt has a flag equivalent (`--title`, `--body`, `--base`, `--head`) so scripting works.
- **Editor escape**: if `--body` is omitted, opens `$EDITOR` for the PR body.
- **`gh browse`** — small but loved feature; opens the resource in a browser.

## Things we should *not* copy

- Extensions system (`gh extension install …`) — complexity we don't need for MVP. Maybe later.
- Aliases — easy to add when there's demand.
- Codespaces / Copilot / Skill — GitHub-only product surface.

## Open questions for the spec session

1. What's our top-level binary name? `bb` is short but collides with the `bb` (Bitwise B) shell builtin on some systems — verify. Alternatives: `bbcli`, `bbk`.
2. Do we need Bitbucket Server (self-hosted, on-prem) support, or Cloud only? They use different APIs.
3. Bitbucket Pipelines — do we want a `bb run`/`bb pipeline` analog in MVP or later?
4. Do we mirror gh's `--json` machinery or use a lighter `-o json|text|tsv` approach?
5. Multi-account support (`gh auth switch`) — useful since people often have a work + personal Bitbucket. Probably yes.
