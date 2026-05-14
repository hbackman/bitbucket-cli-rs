# bitbucket-cli — Research Notes

Background research for designing a `gh`-style CLI for Bitbucket Cloud.

| File | Contents |
| --- | --- |
| [github-cli.md](github-cli.md) | What `gh` is, its command surface, architecture, auth model. The reference implementation we're cloning the shape of. |
| [bitbucket-auth.md](bitbucket-auth.md) | Bitbucket OAuth 2.0 capabilities and constraints; the loopback-redirect trick; storage plan; app-password deprecation timeline. |
| [bitbucket-api.md](bitbucket-api.md) | Bitbucket Cloud REST API basics; endpoints we'll need; conceptual differences from GitHub (decline vs close, workspace vs owner, etc.). |
| [language-choice.md](language-choice.md) | Go vs Rust vs TS, with a recommendation and rationale. |
| [mvp-scope.md](mvp-scope.md) | Tiered command surface, open spec questions, rough phasing. |

## Key findings at a glance

1. **OAuth in a CLI is doable on Bitbucket** despite no PKCE / no device flow — the authorization-code flow with a `http://localhost` loopback redirect (port dynamic at request time) is officially supported and gives near-identical UX to `gh auth login`.
2. **App passwords are dead** (final shutoff 2026-06-09). The non-OAuth scripting path is **API tokens**, equivalent to GitHub PATs.
3. **Go is the recommended language**, primarily because we can use the `gh` source tree directly as a reference for every non-trivial problem.
4. **MVP is ~10 commands** covering auth, repo basics, PR basics, and an `api` escape hatch — enough to use daily.
5. **Biggest concept mismatch**: Bitbucket's "decline" vs GitHub's "close" PR; Bitbucket's per-reviewer approval model vs GitHub's review-as-an-object model. Both are surmountable but affect command shape (`bb pr review --approve` rather than `bb pr review`).
