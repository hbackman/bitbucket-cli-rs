# MVP Scope — Strawman

This is a starting point for the spec conversation. Numbers are rough.

## Tier 1 — actual MVP (the smallest thing that's useful daily)

If someone installs `bb` and these work, they get genuine value:

```
bb auth login                  # OAuth browser flow
bb auth status
bb auth logout
bb auth token
bb auth setup-git              # register git credential helper

bb repo view [WS/REPO]         # show repo info
bb repo clone WS/REPO
bb repo list [WS]

bb pr list                     # current repo, OPEN by default
bb pr view [N]                 # current PR if checked out, else prompt
bb pr create                   # interactive + flags
bb pr checkout N
bb pr diff [N]
bb pr merge [N]
bb pr close [N]                # = decline in Bitbucket-speak
bb pr comment [N]

bb browse [N]                  # open repo/PR in browser
bb api PATH                    # generic API escape hatch
bb config get|set|list
bb --version / --help
```

Cross-cutting:
- `--repo WS/REPO` override on all repo-scoped commands
- `--json field,field` output mode for `list`/`view`/`status`
- `BB_TOKEN` / `BITBUCKET_TOKEN` env var support
- OS keyring storage with plaintext fallback
- TTY detection (color/tables for humans, TSV when piped)

## Tier 2 — fast-follow (probably within the same milestone)

```
bb pr review --approve | --request-changes | --comment
bb pr checks [N]               # build statuses
bb pr ready [N]                # mark draft → ready
bb pr edit [N]
bb repo create
bb repo fork
bb auth refresh
bb auth switch                 # multi-account
bb completion bash|zsh|fish
```

## Tier 3 — post-MVP

- Issues (`bb issue …`) — full surface
- Pipelines (`bb pipeline list/run/view/logs`) — Bitbucket's CI
- Aliases (`bb alias set co 'pr checkout'`)
- Extensions system (skip until we feel the pain of not having it)
- Bitbucket Server (self-hosted) support — different API, big undertaking
- GraphQL (Bitbucket doesn't have one; not applicable)

## Open spec questions

1. **Binary name.** `bb` — short, but possibly collides (`bb` is a Babashka CLI, also a few shells expose `bb` as a builtin or alias). `bbc`? `bbk`? `btb`? Worth a vote.
2. **OAuth consumer ownership.** Ship a public `client_id`/`client_secret` in the binary (like gh does), or require users to create their own consumer? Recommendation: ship public for ease, support `--client-id` override.
3. **Multi-account from day one or later?** gh has `auth switch`. Personal+work Bitbucket is a common split, so probably day-one with a simple "active host+user" pointer.
4. **Output formats.** Just `text` + `--json`? Or also `--template` + `--jq` like gh? Recommendation: start with `text` + `--json`, add `--jq` quickly.
5. **Repo resolution priority.** What overrides what? Proposed: `--repo` flag > `BB_REPO` env > `bb repo set-default` value > `git remote get-url origin` parsing.
6. **Bitbucket Server support in MVP?** Recommendation: **no.** Cloud only. Different API, would double scope. Architect for it but don't ship it.
7. **Distribution.** Homebrew tap + raw GitHub Releases binaries to start. Scoop/winget later. AUR later.
8. **License.** Likely MIT to match the surrounding ecosystem.

## Rough phasing

- **Week 1–2**: skeleton, Cobra command tree, `bb auth login` (OAuth loopback), `bb auth status`, config + keyring storage.
- **Week 3**: API client core, `bb repo view`, `bb repo clone`, repo-from-remote resolution.
- **Week 4**: `bb pr list`, `bb pr view`, `bb pr checkout`, `bb pr diff`.
- **Week 5**: `bb pr create` (the hard one — interactive prompts + editor fallback + branch push).
- **Week 6**: `bb pr merge`, `bb pr close`, `bb pr comment`, `bb api`, `bb browse`, polish, ship 0.1.0.

Tier 2 in a 0.2 release.
