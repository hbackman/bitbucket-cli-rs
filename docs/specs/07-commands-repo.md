# 07 — Repo & Misc Commands

**Status:** Draft (Rust)
**Depends on:** [`01-architecture.md`](01-architecture.md), [`02-authentication.md`](02-authentication.md), [`03-configuration.md`](03-configuration.md), [`04-api-client.md`](04-api-client.md), [`05-output.md`](05-output.md)
**Slice goal:** All `bb repo …` subcommands plus the small utility commands (`bb version`, `bb completion`) work end-to-end.

## Commands in this slice

```
bb repo view        [WS/REPO]   [flags]
bb repo list        [WS]        [flags]
bb repo clone       WS/REPO     [DIR] [-- git-args]
bb repo create      [NAME]                [flags]
bb repo fork        [WS/REPO]              [flags]
bb repo set-default [WS/REPO]              [flags]
bb version
bb completion       <bash|zsh|fish|powershell>
```

> **Dropped:** `bb browse` was scoped here originally but cut to keep the
> surface lean. Re-add if user demand appears. `bb repo view --web` covers the
> common "open this repo" case; everything else (`--pulls`, `--settings`, file
> URLs) was speculative.

## `bb repo view`

```
$ bb repo view workspace/repo
workspace/repo
Public • main branch • 42 stars (members)

  ## README
  rendered markdown ...
```

```
Flags:
  -b, --branch <BRANCH>   Print README for a specific branch
  -w, --web               Open in browser
      --json [<FIELDS>], --jq <EXPR>
```

Body of the response:
- Calls `GET /2.0/repositories/{ws}/{repo}` for metadata.
- Calls `GET /2.0/repositories/{ws}/{repo}/src/{branch}/README.md` (with `.md` / `.rst` / `.txt` fallbacks) for the README. If none exists, skip that section.

Available `--json` fields:
`name, fullName, owner, description, isPrivate, mainBranch, language, createdOn, updatedOn, url, size`.

## `bb repo list`

```
$ bb repo list workspace
NAME                       VISIBILITY  DESCRIPTION                  UPDATED
workspace/api              private     Public-facing API            2 hours ago
workspace/web              private     Web app                      1 day ago
```

If `WORKSPACE` is omitted, list repos accessible to the current user (across all workspaces).

```
Flags:
  -L, --limit <N>        Max repos to fetch (default 30)
      --visibility       Filter: public|private
      --role             Filter by role: owner|admin|member|contributor
      --language         Filter by primary language
      --sort             updated|created|name
      --json, --jq
```

Endpoints:
- `GET /2.0/repositories/{workspace}?role=...&sort=...` when WORKSPACE is provided.
- `GET /2.0/user/permissions/repositories` for "all I can access".

## `bb repo clone`

```
$ bb repo clone workspace/repo
$ bb repo clone workspace/repo my-local-dir
$ bb repo clone workspace/repo -- --depth 1
```

Algorithm:
1. Resolve URL from `git_protocol` (https → `https://bitbucket.org/workspace/repo.git`, ssh → `git@bitbucket.org:workspace/repo.git`).
2. Detect if the source is a fork (Bitbucket: repo has a `parent` field). If so, also add an `upstream` remote pointing at the parent — matches gh behavior.
3. Spawn `git clone <url> [DIR]` via `tokio::process::Command`, then any `--` passthrough args.

```
Flags:
  -u, --upstream-remote-name <NAME>  (default "upstream")
      --no-upstream                  Don't add the upstream remote
```

All args after `--` are passed through to `git clone` directly via clap's `trailing_var_arg`.

## `bb repo create`

Two modes:

1. **Create from local directory** (we have code; need a repo):
   ```
   bb repo create my-new-repo --source=. --push
   ```
2. **Create empty repo**:
   ```
   bb repo create my-new-repo --description "..." --private
   ```

```
Flags:
  -d, --description <TEXT>
      --homepage <URL>
      --public                          # public (default if neither given)
      --private
      --source <DIR>                    # local dir to push
      --push                            # push the source after create
      --remote <NAME>                   # remote name to add (default "origin")
      --team <WORKSPACE>                # defaults to current user's workspace
      --clone                           # after create, clone into PWD
      --add-readme
      --gitignore <TEMPLATE>
      --license <TEMPLATE>
      --default-branch <NAME>           # default "main"
```

`--public` and `--private` are mutually exclusive via clap `ArgGroup`.

POST `/2.0/repositories/{ws}/{slug}` with body. On `--source --push`: `git remote add` + `git push -u`.

Interactive mode (no NAME provided): prompts for workspace, name, description, public/private.

## `bb repo fork`

```
bb repo fork [WS/REPO]
```

Bitbucket forking: `POST /2.0/repositories/{ws}/{repo}/forks` with workspace + name in body.

```
Flags:
      --org <WORKSPACE>          # target workspace (defaults to current user's)
      --fork-name <NAME>         # new repo name (default: source name)
      --clone                    # clone the fork after creating
      --remote                   # add the fork as a remote (default true if cloned)
      --remote-name <NAME>       # default "origin"
      --default-branch-only      # fork only the default branch (Bitbucket: body flag)
```

Algorithm:
1. POST the forks endpoint.
2. Poll `GET /2.0/repositories/{ws}/{slug}` until the fork's `created_on` is set (Bitbucket fork creation is async; usually <2s). Use `tokio::time::sleep(Duration::from_millis(500))` between polls; cap at 30s.
3. If inside a clone of the parent repo and `--remote=true`: `git remote rename origin upstream; git remote add origin <fork-url>`.
4. If `--clone`: clone into PWD.

## `bb repo set-default`

Spec lives in [`03-configuration.md`](03-configuration.md). Implementation lives here:

```
src/cli/repo/set_default.rs
```

Writes `bb.default-repo = workspace/slug` via `git config --local` (preferred — per-clone). Falls back to `default_repo:` in `config.yml` when not in a git repo.

```
Flags:
      --view       Print the current default
      --unset      Remove the default
```

Interactive mode: read all Bitbucket remotes from the current git repo and prompt the user to select one.

## `bb version`

Already partially implemented in spec 01. Extend to two output modes:

```
$ bb version
bb 0.1.0 (commit deadbeef, built 2026-05-14)
https://github.com/hbackman/bitbucket-cli/releases/tag/v0.1.0
```

`bb version --json` prints structured info:
```json
{ "version": "0.1.0", "commit": "deadbeef", "date": "2026-05-14" }
```

Also performs an update check unless `BB_NO_UPDATE_NOTIFIER` is set or `CI=true`.

### Update notifier

- Cached at `${XDG_CACHE_HOME}/bb/update-check.json` with timestamp.
- Once per 24h: fetch `https://api.github.com/repos/hbackman/bitbucket-cli/releases/latest`, compare tag (semver via `semver` crate).
- If newer, print a one-line notice to stderr after the version output. Don't block.
- Detect install method via `std::env::current_exe()` path:
  - Under `/opt/homebrew/` or `/usr/local/Cellar/` → suggest `brew upgrade hbackman/bb/bb`.
  - Under `~/.cargo/bin` → suggest `cargo install bb-cli --force` (TBD name).
  - Otherwise → link to releases page.

Wire the notifier into the root command so it runs after every interactive invocation (silent in non-TTY).

## `bb completion`

```
bb completion bash > /etc/bash_completion.d/bb
bb completion zsh > "${fpath[1]}/_bb"
bb completion fish > ~/.config/fish/completions/bb.fish
```

Powered by `clap_complete` (`generate(shell, &mut cmd, "bb", &mut stdout)`).

```
Flags:
  <SHELL>   bash | zsh | fish | powershell | elvish
```

## File layout

```
src/cli/repo/
├── mod.rs                    # RepoArgs + RepoCommand enum, dispatch
├── view.rs
├── list.rs
├── clone.rs
├── create.rs
├── fork.rs
├── set_default.rs
└── display.rs

src/cli/version.rs            # already in place; extended with --json + update check
src/cli/completion.rs         # new

src/update/
├── mod.rs                    # notifier: cache + fetch + compare
└── tests.rs
```

The single-file `src/cli/repo.rs` stub from spec 01 is promoted to a directory module here.

## Required dependencies (deferred from spec 01)

```toml
clap_complete = "4"
semver        = "1"
```

(`webbrowser` is pulled in by spec 02 for the OAuth flow and reused by `bb
repo view --web`.)

## Tests

- `repo view`: README fallback (md → rst → txt → none).
- `repo list`: pagination, filter combinations.
- `repo clone`: URL synthesis from `git_protocol`; `upstream` remote added when fork.
- `repo create`: each flag combination produces correct body; `--source --push` invokes git correctly.
- `repo fork`: poll loop; `git remote` mutation.
- `version`: update notifier respects cache + `BB_NO_UPDATE_NOTIFIER`.
- `completion`: each shell flag produces non-empty output.

## Concrete deliverables

1. All modules above with full implementations.
2. Update notifier under `src/update/`.
3. `clap_complete`-backed completion for all supported shells.
4. Wire `repo`, `version`, `completion` into `src/cli/mod.rs` (replacing the stubs).
5. Tests as listed above.

## Acceptance criteria

- `bb repo view`, `bb repo list`, `bb repo clone <ws/r>`, `bb repo create my-test --private`, `bb repo fork <ws/r>`, `bb repo set-default <ws/r>` all work against live Bitbucket Cloud.
- `bb version` prints version and triggers a check at most once per 24h.
- `bb completion zsh > /tmp/_bb && source /tmp/_bb && bb pr <Tab>` offers PR subcommands.
- All commands support `--json` + `--jq` where listed.

## Open questions

- Whether `bb repo clone` should set up `upstream` for *all* forks by default (gh's behavior) or require a flag. **Lean: gh-parity**, set it up by default with `--no-upstream` to skip.
- Whether the update notifier should detect cargo / homebrew install paths and message accordingly. **Lean: yes** — covered above.
