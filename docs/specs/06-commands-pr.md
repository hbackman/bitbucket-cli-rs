# 06 — PR Commands

**Status:** Draft (Rust)
**Depends on:** [`01-architecture.md`](01-architecture.md), [`02-authentication.md`](02-authentication.md), [`03-configuration.md`](03-configuration.md), [`04-api-client.md`](04-api-client.md), [`05-output.md`](05-output.md)
**Slice goal:** Every PR command listed below works end-to-end against Bitbucket Cloud. This is the largest single command surface in MVP.

## Commands in this slice

```
bb pr list      [flags]
bb pr view      [N] [flags]
bb pr status              [flags]
bb pr create              [flags]
bb pr checkout  N         [flags]
bb pr diff      [N]       [flags]
bb pr merge     [N]       [flags]
bb pr close     [N]       [flags]      # = decline
bb pr reopen    [N]       [flags]
bb pr comment   [N]       [flags]
bb pr review    [N]       [flags]
bb pr checks    [N]       [flags]
bb pr ready     [N]       [flags]
bb pr edit      [N]       [flags]
```

## Shared concepts

### Resolving the target PR

When `[N]` is omitted, resolve in this order:
1. The PR (open or merged) whose source branch == `git symbolic-ref --short HEAD`. Single match required; if multiple, list them and exit with `CliError::Flag`.
2. Otherwise: `CliError::Flag("no pull request specified — provide a PR number or branch name")`.

A helper `pr_finder::find(ctx, args, opts)` in `src/cli/pr/finder.rs` does this. Used by every command that takes `[N]`.

### `bb pr` parent

```rust
// src/cli/pr/mod.rs
#[derive(clap::Args, Debug)]
pub struct PrArgs {
    #[command(subcommand)]
    pub command: PrCommand,
}

#[derive(clap::Subcommand, Debug)]
pub enum PrCommand {
    List(list::Args),
    View(view::Args),
    Status(status::Args),
    Create(create::Args),
    Checkout(checkout::Args),
    Diff(diff::Args),
    Merge(merge::Args),
    Close(close::Args),
    Reopen(reopen::Args),
    Comment(comment::Args),
    Review(review::Args),
    Checks(checks::Args),
    Ready(ready::Args),
    Edit(edit::Args),
}

pub async fn run(args: PrArgs, ctx: &mut Context) -> Result<(), CliError> {
    match args.command {
        PrCommand::List(a)    => list::run(a, ctx).await,
        /* ... */
    }
}
```

This replaces the spec-01 stub.

## `bb pr list`

List PRs in the target repo.

```
Flags:
  -s, --state <STATE>     State filter: open|merged|declined|all (repeatable; default open)
  -a, --author <USER>     Filter by author username (or "@me")
  -A, --assignee <USER>   Filter by assignee (Bitbucket: reviewer)
  -B, --base <BRANCH>     Filter by destination branch
  -H, --head <BRANCH>     Filter by source branch
  -L, --limit <N>         Maximum number of PRs to fetch (default 30)
      --reviewer <USER>   Filter by reviewer username (or "@me")
      --query <BBQL>      Bitbucket BBQL query string passed through `?q=`
  -w, --web               Open list in browser instead
      --json [<FIELDS>]   JSON output mode (see --json with no value for available fields)
      --jq <EXPR>         Filter JSON with jq
```

Columns (table):

```
ID    TITLE                        BRANCH                       UPDATED
#42   Fix login flow               feature/login                about 2 hours ago
#41   Bump deps                    deps/upgrade                 1 day ago
```

State icon to the left of ID for non-OPEN states (`✓` merged magenta, `✗` declined red).

Available `--json` fields: `id, title, body, state, author, sourceBranch, destinationBranch, createdOn, updatedOn, url, draft, reviewers, closeSourceBranch`.

Bitbucket API: `GET /2.0/repositories/{ws}/{repo}/pullrequests?state=...&q=...&pagelen=N`. Multi-state requires multiple queries; aggregate client-side using `futures::future::join_all`.

`@me` resolves to the current user via `ctx.api()?.user().me().await?`.

## `bb pr view`

Print detail for a single PR.

```
$ bb pr view 42
Fix login flow workspace/repo#42
Open • opened about 2 hours ago by hbackman • 12 commits • 3 files changed
Source branch: feature/login → main
Reviewers: alice (approved), bob (changes requested)
Build: 1 success, 1 failed
URL: https://bitbucket.org/workspace/repo/pull-requests/42

  ## Summary
  Fixes the broken redirect when the session cookie is missing.

  ## Test plan
  - Manual login + logout
  - Unit tests covering the fallback path
```

```
Flags:
  -c, --comments         Print comments instead of body
  -w, --web              Open in browser
      --json [<FIELDS>], --jq <EXPR>
```

Body rendered via `pulldown-cmark` parsed into a minimal ANSI styler: bold headings, light emphasis, fenced code blocks indented. Full markdown rendering is out of scope for MVP; we keep the formatter behind a tiny `render_markdown(s: &str, cs: &ColorScheme) -> String` helper so it's easy to swap.

## `bb pr status`

```
$ bb pr status
Current branch
  #42 Fix login flow [feature/login] - Approved by alice, 2 builds passing

Created by you
  #41 Bump deps [deps/upgrade]

Requesting a code review from you
  #38 Add observability hooks [team/observability]
  #29 Tweak retries [chore/retries]
```

Sections:
1. Current branch (the PR for the current branch, if any).
2. Created by you (open PRs you authored).
3. Requesting a code review from you (open PRs where you're a reviewer and haven't approved).

Each section issues a separate `pull_requests().list(...)` call with the appropriate `q=` filter; results are joined with `tokio::join!`.

## `bb pr create`

The most complex command.

```
Flags:
  -t, --title <TITLE>
  -b, --body <BODY>
  -F, --body-file <PATH>          ("-" for stdin)
  -B, --base <BRANCH>             Destination branch (defaults to repo mainbranch)
  -H, --head <BRANCH>             Source branch (defaults to current branch)
  -d, --draft
  -r, --reviewer <USER>           Repeatable
      --no-close-source-branch
      --close-source-branch
  -f, --fill                      Use commit info to fill title and body
      --fill-first                Use first commit only for title and body
  -w, --web                       Open the create form in browser instead
      --editor                    Open editor for title + body even if --title/--body given
      --dry-run                   Print the payload that would be sent; do not POST
      --recover <PATH>            Path to a JSON file from a previous failed run, to retry
```

### Algorithm

1. Determine source (head) and base (target) branches.
2. Check that head branch is pushed to a Bitbucket remote. If not:
   - Detect remote that points to a Bitbucket repo.
   - Prompt "Push <branch> to <remote>?" (skip prompt if `-y` or non-interactive — then auto-push).
   - `git::push(remote, branch)` (already exposed in spec 01).
3. Compute defaults:
   - `--fill`: read commits between `base..head` (via `git log`); first commit subject as title; combined commit bodies as PR body.
   - `--fill-first`: title + body from first commit only.
4. Prompt for missing required fields (TTY only):
   - Title (required)
   - Body (open editor; allow empty)
   - Reviewers (optional; offer suggestions from `getEffectiveDefaultReviewers` endpoint)
   - Confirm action: Submit / Submit as draft / Edit again / Open in browser / Cancel
5. POST `/2.0/repositories/{ws}/{repo}/pullrequests` with the assembled body.
6. On success: print PR URL.
7. On failure: write the payload to `${XDG_CACHE_HOME}/bb/pr-create-recover-<timestamp>.json` and print: "Run `bb pr create --recover <path>` to retry."

### Default reviewers

Bitbucket exposes `getEffectiveDefaultReviewers` — call it and pre-fill the reviewer list (interactive mode only, as defaults).

### `--draft`

Bitbucket's API supports `draft: true` on create. Drafts cannot be merged until `bb pr ready` lifts them.

## `bb pr checkout`

```
bb pr checkout N

Flags:
  -b, --branch <NAME>      Local branch name (default: source branch name)
      --recurse-submodules
      --force              Reset local branch to remote
      --detach             Checkout in detached HEAD
```

Algorithm:
1. Fetch the PR via API.
2. `git fetch <remote> <source-branch>:refs/remotes/<remote>/<source-branch>`.
3. If a local branch with the target name already exists, `git checkout` it and `git merge --ff-only` (or `git reset --hard` if `--force`).
4. Otherwise, `git checkout -b <branch> --track <remote>/<source-branch>` (already exposed as `git::create_branch_tracking`).

If the PR is from a fork (Bitbucket supports this), add the fork as a temporary remote (`bb-fork-<id>`) and pull from it. Cleanup of the temp remote on `bb pr checkout`'s next invocation against a different PR.

## `bb pr diff`

```
bb pr diff [N] [--color always|auto|never] [--web]
```

Fetches `GET /2.0/repositories/{ws}/{repo}/pullrequests/{id}/diff`, pipes through `delta` / `diff-so-fancy` if found on `PATH`, otherwise renders raw diff with line-prefix coloring (green `+`, red `-`, cyan `@@`).

Pager: yes, via the IoStreams pager hook from spec 05.

## `bb pr merge`

```
Flags:
  -m, --merge        Use merge-commit strategy (default)
  -s, --squash       Use squash strategy
  -r, --rebase       Use fast-forward strategy (Bitbucket: "fast-forward"; effectively rebase + ff)
  -d, --delete-branch
      --auto         Set up auto-merge once approvals & checks land
                     (NOTE: Bitbucket has no native auto-merge; we error with a clear message)
  -b, --body <TEXT>          Custom merge commit body
  -t, --subject <TEXT>       Custom merge commit subject
      --close-source-branch  Override the close-source flag at merge time
```

Algorithm:
- Resolve PR.
- POST `/2.0/repositories/{ws}/{repo}/pullrequests/{id}/merge` with `merge_strategy`, `message`, `close_source_branch`.
- On success: print "✓ Merged pull request #N (workspace/repo)".

`--auto`: surface `CliError::Flag("Bitbucket does not support native auto-merge. Use Bitbucket's auto-merge UI or rerun this command once requirements are met.")`.

## `bb pr close` (decline)

```
Flags:
  -c, --comment <TEXT>      Add a comment before declining
  -d, --delete-branch       Delete the source branch (local + remote)
```

POST `/2.0/repositories/{ws}/{repo}/pullrequests/{id}/decline`. If `--comment` is set, post the comment first, then decline.

## `bb pr reopen`

Bitbucket has no direct "reopen" endpoint for declined PRs — declined PRs are terminal. If the user asks for reopen on a declined PR, surface `CliError::Flag("Bitbucket does not support reopening declined pull requests. Create a new PR from the same branch.")`.

(Listing the command anyway so the gh-parity expectation is handled with a useful message.)

## `bb pr comment`

```
Flags:
  -b, --body <TEXT>
  -F, --body-file <PATH>     ("-" for stdin)
      --edit-last            Replace your most recent comment
      --create-if-none
      --delete-last
```

POST `/2.0/repositories/{ws}/{repo}/pullrequests/{id}/comments` with `{"content":{"raw":"..."}}`.

Inline (per-file/per-line) comments via `--inline FILE:LINE` are post-MVP — flag dropped in MVP.

## `bb pr review`

Composed from primitives:

```
Flags (mutually exclusive):
  -a, --approve
  -r, --request-changes
  -c, --comment
      --undo-approve
      --undo-request-changes
  -b, --body <TEXT>
  -F, --body-file <PATH>
```

Use `clap`'s `ArgGroup(.. required(true), .. multiple(false))` to enforce mutual exclusion.

Bitbucket has separate endpoints for approve, request changes, and comment. Map:

| Flag | Endpoint |
| --- | --- |
| `--approve` | `POST .../pullrequests/{id}/approve` |
| `--undo-approve` | `DELETE .../pullrequests/{id}/approve` |
| `--request-changes` | `POST .../pullrequests/{id}/request-changes` |
| `--undo-request-changes` | `DELETE .../pullrequests/{id}/request-changes` |
| `--comment` | `POST .../pullrequests/{id}/comments` |

If `--approve` or `--request-changes` is combined with `--body`, post the comment first.

## `bb pr checks`

Lists build statuses attached to the PR's head commit.

```
$ bb pr checks
✓ ci/test         (passed)  https://...
✗ ci/lint         (failed)  https://...
- ci/deploy       (pending) https://...
```

GET `/2.0/repositories/{ws}/{repo}/pullrequests/{id}/statuses`.

Flags:
- `-w, --web` open in browser
- `--watch` poll until all statuses are terminal (configurable interval, default 5s, max 30 minutes)
- `--interval <DURATION>` (e.g. `5s`, `30s`) — paired with `--watch`
- `--required` only show statuses required by branch protection (post-MVP — Bitbucket's branching model API)

Exit code:
- 0 if all statuses pass
- 1 if any fail or remain pending after the watch window

`--watch` uses `tokio::time::interval`.

## `bb pr ready`

Lift draft status. PUT `/pullrequests/{id}` with `{"draft":false}`.

## `bb pr edit`

```
Flags:
  -t, --title <TEXT>
  -b, --body <TEXT>
  -F, --body-file <PATH>
  -B, --base <BRANCH>
      --add-reviewer <USER>      Repeatable
      --remove-reviewer <USER>   Repeatable
```

PUT `/2.0/repositories/{ws}/{repo}/pullrequests/{id}` with the patched fields.

Source branch cannot be edited via API; the flag is rejected if present.

## File layout

```
src/cli/pr/
├── mod.rs                      # PrArgs + PrCommand subcommand enum, dispatch
├── finder.rs                   # resolve PR by N or by current-branch
├── display.rs                  # state colors, icons, time formatting
├── types.rs                    # local view models for table rendering
├── list.rs
├── view.rs
├── status.rs
├── create/
│   ├── mod.rs
│   ├── fill.rs                 # --fill commit reading
│   └── recover.rs              # --recover JSON loading
├── checkout.rs
├── diff.rs
├── merge.rs
├── close.rs
├── reopen.rs
├── comment.rs
├── review.rs
├── checks.rs
├── ready.rs
└── edit.rs
```

This replaces the single-file stub `src/cli/pr.rs` from spec 01 by promoting it to a directory module.

## Tests

Each subcommand gets:
- A unit test that constructs `Context::test()` with a mock prompter and `wiremock` server, then asserts on the API calls made + stderr/stdout output.
- Fixtures for Bitbucket JSON responses in `src/cli/pr/<sub>/fixtures/` (loaded via `include_str!`).

Cover at minimum:
- `list`: state filter, JSON output, field validation, `@me` expansion.
- `view`: JSON output, `--comments` flag.
- `create`: `--fill`, push prompt, draft, dry-run, recover.
- `checkout`: same-repo and fork cases.
- `merge`: each strategy maps to the right `merge_strategy`.
- `close`: with and without `--comment`.
- `review`: each flag hits the right endpoint.
- `checks`: aggregated status; exit codes for pass/fail.

## Concrete deliverables

1. All subcommand modules listed above, fully implemented.
2. `src/cli/pr/finder.rs` with PR resolution (number + branch).
3. `src/cli/pr/display.rs` with state-colors, icons, and a reusable PR-row renderer.
4. Tests for every subcommand.
5. Replace the stub registration in `src/cli/mod.rs` with the new `PrArgs`/`PrCommand` types.

## Acceptance criteria

For each command, the acceptance criterion is: running it against a recorded fixture or live Bitbucket Cloud workspace produces the documented behavior, and the JSON output mode produces output `jq` can navigate cleanly.

Specific scripted-use scenarios that must work:

- `bb pr list --json id,title --jq '.[] | "\(.id) \(.title)"'`
- `bb pr view 42 --json author --jq '.author.username'`
- `bb pr checkout 42 && bb pr diff && bb pr comment -b "looks good"`
- `bb pr create --title "x" --body "y" --base main --head feature/x --draft`
- `bb pr review 42 --approve --body "lgtm"`
- `bb pr merge 42 --squash --delete-branch`
- `bb pr close 42 --comment "won't fix"`

## Open questions

- Inline (per-line) PR comments — add to `bb pr comment` in 0.1 or defer? **Lean: defer to 0.2.**
- `bb pr checks --watch` interval — fixed or configurable? **Lean: configurable via `--interval`, default 5s.**
- Whether `bb pr create` should detect "head branch is on a fork and you have push access to the upstream too" and prompt where to push — gh has elaborate handling. **Lean: simpler MVP** — assume push to the configured remote on the current branch.
- Whether `bb pr view --comments` should print inline review comments too (which Bitbucket models separately). **Lean: yes**, all comment kinds in one stream.
- Markdown renderer for `bb pr view` body — `pulldown-cmark` + custom ANSI vs a richer crate like `termimad`. **Lean: pulldown-cmark + custom**, keeps the dep tree small.
