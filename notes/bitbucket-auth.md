# Bitbucket Cloud Authentication — Research Notes

## TL;DR

- **OAuth 2.0 authorization-code flow with a localhost loopback redirect works** for a CLI. We can build a `bb auth login` that opens the browser, listens on a random localhost port, and captures the code. This is the recommended path for MVP.
- **Bitbucket does NOT support PKCE or the OAuth device-authorization grant** — so we can't do the "show a code, type it in the browser" flow that gh uses. But the loopback flow gives equivalent UX.
- **App passwords are deprecated** — creation disabled since 2025-09-09, full shutoff 2026-06-09. We should *not* build around them.
- **API tokens** (the modern replacement) are user-created, scope-limited, workspace-scopable, and the recommended fallback for headless/CI use. Equivalent to a GitHub PAT.

## What Bitbucket Cloud OAuth 2.0 supports

Sources: Atlassian OAuth on Bitbucket Cloud docs; Atlassian Community confirmations on localhost redirects.

| Grant type | Supported? | Notes |
| --- | --- | --- |
| Authorization Code | ✅ | The full 3-legged flow. Requires `client_id` + `client_secret`. |
| Implicit | ✅ | Browser-only, returns token in URL fragment. Not useful for CLI. |
| Resource Owner Password Credentials | ❌ | Removed. |
| Client Credentials | partial | Available but auth as the consumer, not as a user. Limited. |
| Device Authorization Grant | ❌ | Not supported. |
| PKCE | ❌ | Not supported (confirmed in Atlassian community posts). |
| JWT Token Exchange | ✅ | Atlassian-specific, for Connect apps. Not relevant here. |

Token lifetime: **access tokens expire after 2 hours**; **refresh tokens** are issued and let us mint new access tokens without re-prompting.

## The loopback-redirect trick

The key finding: when registering an OAuth consumer, set the callback URL to `http://localhost` (path optional, **no port**). At authorize-request time, append the actual port dynamically:

```
https://bitbucket.org/site/oauth2/authorize
  ?client_id=CLIENT_ID
  &response_type=code
  &redirect_uri=http://localhost:PORT
```

Bitbucket's loopback handling is special-cased to ignore the port mismatch with the registered callback. This is what lets a CLI bind to an ephemeral port, spawn the browser, and receive the code.

Flow for `bb auth login`:
1. Generate a random `state`.
2. Bind a local HTTP listener on a random port (e.g. `127.0.0.1:0`).
3. Open the user's browser to the authorize URL with `redirect_uri=http://localhost:<port>` and the state.
4. User logs in + authorizes in browser; Bitbucket redirects back to `http://localhost:<port>/?code=…&state=…`.
5. CLI exchanges the code for an access + refresh token at `https://bitbucket.org/site/oauth2/access_token` (POST, basic auth using `client_id:client_secret`).
6. Store tokens in the OS keyring.

## The "where do we get a client_id from" problem

Standard OAuth public clients can't really keep a `client_secret` secret — it ships in the binary. But Atlassian doesn't (as of the docs I read) require PKCE, so they expect a secret. Two options:

1. **Ship a public `client_id` + `client_secret`** baked into the binary. This is the same approach `gh` uses for GitHub (their secret is technically in the binary too). The "secret" is more of a per-app identifier than a real secret in this model.
2. **Require each user to create their own OAuth consumer** in their Bitbucket workspace settings, then run `bb auth login --client-id … --client-secret …` once. More setup friction but no risk of our shared client being throttled or revoked.

Recommendation: **option 1** for MVP (matches gh UX), with option 2 supported as a fallback for users in regulated environments. We should also be ready to rotate our embedded client if abuse becomes an issue.

## API tokens — the recommended scripting/CI path

Bitbucket's [API tokens docs](https://support.atlassian.com/bitbucket-cloud/docs/api-tokens/) describe user-created tokens with:
- Per-token scope selection.
- Optional workspace restriction.
- No 2FA prompt on use.

These replace app passwords. For MVP we should:
- Accept a token via env var (`BB_TOKEN` and `BITBUCKET_TOKEN`, mirroring gh's `GH_TOKEN`/`GITHUB_TOKEN`).
- Support `bb auth login --with-token` (read from stdin), same as gh.

## Scope inventory

Common scopes a CLI tool would want:
- `account` — basic user info
- `repository` / `repository:write` / `repository:admin`
- `pullrequest` / `pullrequest:write`
- `issue` / `issue:write`
- `pipeline` / `pipeline:write` / `pipeline:variable`
- `webhook`

We should request a sensible default scope set on `bb auth login`, with a `--scopes` override.

## `bb auth setup-git`

Same idea as `gh auth setup-git`: register `bb` as a git credential helper so HTTPS pushes/pulls authenticate with the stored token. Concretely:

```
[credential "https://bitbucket.org"]
    helper = !bb auth git-credential
```

Where `bb auth git-credential` reads `get`/`store`/`erase` on stdin per the [git-credential protocol](https://git-scm.com/docs/git-credential).

## Storage

Mirror gh exactly:

1. `BB_TOKEN` / `BITBUCKET_TOKEN` env vars override everything.
2. OS keyring (`keyring`/`go-keyring` in Go, `keyring` crate in Rust). macOS Keychain, Windows Credential Manager, libsecret on Linux.
3. Plaintext fallback at `~/.config/bb/hosts.yml` when keyring is unavailable, with a clear warning.

Schema sketch (`hosts.yml`):
```yaml
bitbucket.org:
  user: hbackman
  oauth_token: ...
  refresh_token: ...
  token_expires_at: 2026-05-14T13:00:00Z
  client_id: ...           # if user provided their own consumer
```
