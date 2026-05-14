# 02 — Authentication

**Status:** Draft (Rust)
**Depends on:** [`01-architecture.md`](01-architecture.md), [`03-configuration.md`](03-configuration.md)
**Slice goal:** `bb auth login` runs an OAuth browser flow, stores tokens in the OS keyring (or plaintext fallback), and `bb auth status` reflects the result. `bb auth logout`, `bb auth token`, `bb auth switch`, `bb auth setup-git`, and `bb auth git-credential` all work. Token refresh happens transparently on 401 from the API client — no user-facing `refresh` command (re-run `login` if the refresh-token-expired path triggers).

## Background you need to know

- Bitbucket Cloud supports **OAuth 2.0 authorization-code flow only** — no PKCE, no device flow.
- Bitbucket's loopback handling is special-cased: register the OAuth consumer's callback as `http://localhost` (no port, no path), then send `redirect_uri=http://localhost:<port>` at authorize time. Bitbucket ignores the port mismatch.
- Access tokens expire after **2 hours**. Refresh tokens are issued and have no documented expiry.
- App passwords are deprecated (final cutoff 2026-06-09). **Do not** build any flow that creates or relies on them.
- API tokens are the modern user-scoped credential (per-token scope, optional workspace restriction). Treat them like GitHub PATs — accept via env or `--with-token`, never create them on the user's behalf.

## OAuth consumer

We register a public OAuth consumer in a Bitbucket workspace owned by the project. Callback URL: `http://localhost` (no port).

Client ID and secret are baked into the release binary via compile-time env vars threaded through `build.rs`:

```rust
// build.rs — additions on top of the version-stamping block from spec 01
let client_id = std::env::var("BB_OAUTH_CLIENT_ID").unwrap_or_default();
let client_secret = std::env::var("BB_OAUTH_CLIENT_SECRET").unwrap_or_default();
println!("cargo:rustc-env=BB_EMBEDDED_OAUTH_CLIENT_ID={client_id}");
println!("cargo:rustc-env=BB_EMBEDDED_OAUTH_CLIENT_SECRET={client_secret}");
println!("cargo:rerun-if-env-changed=BB_OAUTH_CLIENT_ID");
println!("cargo:rerun-if-env-changed=BB_OAUTH_CLIENT_SECRET");
```

Release builds set these in CI (see [`08-distribution.md`](08-distribution.md)). Local dev builds leave them empty — `bb auth login` then requires `--client-id` / `--client-secret` (or the env-var overrides) explicitly.

Runtime override precedence:
1. `--client-id` / `--client-secret` flags on `bb auth login`.
2. `BB_OAUTH_CLIENT_ID` / `BB_OAUTH_CLIENT_SECRET` env vars.
3. Embedded values from `env!("BB_EMBEDDED_OAUTH_CLIENT_ID")` (empty → fall through to error).

## Endpoints

- Authorize: `https://bitbucket.org/site/oauth2/authorize`
- Token: `https://bitbucket.org/site/oauth2/access_token`
- Current user (for displaying username after login): `GET https://api.bitbucket.org/2.0/user`

## Default scopes

Request these by default on `bb auth login`:

```
account
repository
repository:write
pullrequest
pullrequest:write
issue
webhook
```

Override with `--scopes "scope1,scope2,..."`. We do **not** request `repository:admin` or `pipeline:variable` by default — those need an explicit opt-in.

## The login flow (`bb auth login`)

### Subcommand surface

```
bb auth login [flags]

Flags:
      --hostname <HOST>         Bitbucket hostname (default "bitbucket.org")
      --web                     Use browser OAuth flow (default)
      --with-token              Read API token from stdin
      --scopes <SCOPES>         Comma-separated scopes (default: see above)
      --client-id <ID>          Override the OAuth client_id
      --client-secret <SECRET>  Override the OAuth client_secret
      --insecure-storage        Force plaintext storage (skip keyring)
      --git-protocol <PROTO>    Default protocol for `bb repo clone` / `auth setup-git` (https|ssh)
      --no-setup-git            Skip the `setup-git` prompt at the end
```

### Interactive flow when no flags given

```
$ bb auth login
? Authenticate to which Bitbucket host? bitbucket.org
? How would you like to authenticate? [Use arrows]
  > Login with a web browser
    Paste an API token
? Default git protocol [https/ssh]: https
? Authenticate Git with your Bitbucket credentials? Yes
- Opening https://bitbucket.org/site/oauth2/authorize?... in your browser.
- Authenticate in your browser, then return here.
✓ Authentication complete.
✓ Logged in to bitbucket.org as <username> (keyring)
✓ Configured git protocol https
✓ Git operations on bitbucket.org configured to use bb as the credential helper
```

### Browser flow algorithm

```rust
pub async fn login_via_browser(ctx: &mut Context, opts: &LoginOptions) -> Result<()> {
    // 1. Bind a tokio TCP listener on a random local port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    // 2. Build authorize URL using the oauth2 crate.
    let state = CsrfToken::new_random();
    let redirect_uri = format!("http://localhost:{port}");
    let client = oauth_client(opts, &redirect_uri)?;
    let (auth_url, _csrf) = client
        .authorize_url(|| state.clone())
        .add_scopes(opts.scopes.iter().map(|s| Scope::new(s.clone())))
        .url();

    // 3. Open the browser, print URL as fallback.
    writeln!(ctx.io.err(), "- Opening {auth_url} in your browser.")?;
    if let Err(e) = webbrowser::open(auth_url.as_str()) {
        writeln!(ctx.io.err(), "  Could not open browser ({e}). Visit the URL above manually.")?;
    }

    // 4. Wait for the callback on the listener (5-minute hard timeout).
    let code = await_callback(listener, state.secret(), Duration::from_secs(300)).await?;

    // 5. Exchange the code for tokens.
    let tokens = client
        .exchange_code(AuthorizationCode::new(code))
        .request_async(async_http_client)
        .await?;

    // 6. Fetch current user to learn the display username + storage key.
    let user = fetch_user(ctx, tokens.access_token().secret()).await?;

    // 7. Persist (keyring + hosts.yml).
    store_auth(&opts.hostname, &user.username, &tokens, opts.use_insecure_storage)?;
    Ok(())
}
```

`oauth_client` builds an `oauth2::basic::BasicClient` configured against Bitbucket's authorize/token endpoints. `async_http_client` is the `oauth2::reqwest::async_http_client` adapter using our shared `reqwest::Client` (pulled from `ctx.http()` so it inherits the user agent and debug logging from spec 04).

### `await_callback` HTTP handler

A tiny single-request HTTP server bound to the listener from step 1. We do **not** pull in axum/warp/hyper-full — the protocol surface is one GET, parsed by hand:

```rust
async fn await_callback(
    listener: tokio::net::TcpListener,
    expected_state: &str,
    timeout: Duration,
) -> Result<String> {
    let accept = async {
        let (mut socket, _) = listener.accept().await?;
        let req = read_http_request(&mut socket).await?;          // read until \r\n\r\n
        let (path, query) = parse_request_line(&req)?;
        let resp = match (path.as_str(), query.get("code"), query.get("state"), query.get("error")) {
            ("/", Some(code), Some(state), _) if state == expected_state => {
                write_html(&mut socket, 200, SUCCESS_PAGE).await?;
                Ok(code.clone())
            }
            ("/", _, _, Some(err)) => {
                write_html(&mut socket, 400, &error_page(err, query.get("error_description"))).await?;
                Err(anyhow!("oauth callback error: {err}"))
            }
            _ => {
                write_html(&mut socket, 404, "Not found.").await?;
                Err(anyhow!("unexpected callback path {path:?}"))
            }
        };
        resp
    };
    tokio::time::timeout(timeout, accept)
        .await
        .map_err(|_| anyhow!("authentication timed out. Re-run `bb auth login`."))?
}
```

Hard timeout: 5 minutes.

### Token storage

Layered, in priority order on read:

1. `BB_TOKEN` env var (highest, never persisted to keyring).
2. `BITBUCKET_TOKEN` env var (alias).
3. OS keyring (via the `keyring` crate).
4. Plaintext fallback at `$XDG_CONFIG_HOME/bb/hosts.yml` (mode 0600).

Keyring service name: `bb:bitbucket.org` (per-host). Keyring username: the Bitbucket account name. Stored value: a JSON blob with the token, refresh token, expiry, scopes, and oauth client identifier.

When `--insecure-storage` is passed or the keyring is unavailable, write to `hosts.yml`. Emit a warning on stderr.

### `hosts.yml` schema

```yaml
bitbucket.org:
  active_user: hbackman
  users:
    hbackman:
      type: oauth                    # or "api_token" or "env"
      oauth_token: ...               # only present when --insecure-storage
      refresh_token: ...
      token_expires_at: 2026-05-14T13:00:00Z
      scopes: [account, repository, pullrequest, ...]
      git_protocol: https
    work-account:
      type: oauth
      ...
```

When the keyring is used, only `active_user`, per-user `type`, `scopes`, `git_protocol`, and `token_expires_at` are stored in `hosts.yml`. The token blob lives in the keyring.

## API token flow

```
$ bb auth login --with-token < ~/my-bitbucket-token
```

Reads stdin to EOF, calls `GET /2.0/user` with `Authorization: Bearer <token>` to validate and learn the username, then stores with `type: api_token`. No refresh logic — API tokens don't refresh.

Validation: if the token returns 401 or lacks the requested scopes (best-effort check), surface a clear error.

## Token refresh

The API client (built in [`04-api-client.md`](04-api-client.md)) asks `src/auth` for a token per request:

```rust
pub struct AuthSource {
    pub client_id: String,
    pub client_secret: String,
    pub hosts: Arc<RwLock<Hosts>>,
    pub keyring: Box<dyn KeyringBackend + Send + Sync>,
    pub http: reqwest::Client,
}

impl AuthSource {
    pub async fn access_token(&self, host: &str, user: Option<&str>) -> Result<String> {
        let mut rec = self.load(host, user).await?;
        match rec.kind {
            AuthKind::ApiToken => Ok(rec.access_token),
            AuthKind::Env     => Ok(rec.access_token),
            AuthKind::OAuth   => {
                let now = OffsetDateTime::now_utc();
                if now + Duration::seconds(30) < rec.expires_at {
                    return Ok(rec.access_token);
                }
                let refreshed = refresh_oauth_token(
                    &self.http,
                    &self.client_id,
                    &self.client_secret,
                    &rec.refresh_token,
                ).await?;
                rec.apply_refresh(refreshed);
                self.store(host, &rec).await?;
                Ok(rec.access_token)
            }
        }
    }

    /// Called by the API client on a 401 — force a refresh even if the cached token looks fresh.
    pub async fn refresh_now(&self, host: &str, user: Option<&str>) -> Result<String> {
        let mut rec = self.load(host, user).await?;
        rec.expires_at = OffsetDateTime::UNIX_EPOCH; // invalidate cache
        self.store(host, &rec).await?;
        self.access_token(host, user).await
    }
}
```

The API client, on a 401 response, calls `refresh_now()` once and retries the request. A second 401 returns `CliError::Auth(...)`.

## Sub-commands

### `bb auth status`

```
$ bb auth status
bitbucket.org
  ✓ Logged in to bitbucket.org as hbackman (keyring)
  - Active user: hbackman
  - Git operations protocol: https
  - Token scopes: 'account', 'repository', 'repository:write', 'pullrequest', 'pullrequest:write', 'issue', 'webhook'
  - Token expires: 2026-05-14T13:00:00Z (in 1h 42m)
```

If `BB_TOKEN` is set, show `(from environment)` instead of `(keyring)`.
Exit code 4 if no host is logged in (`AuthError`).

Flags:
- `--hostname <HOST>` filter to one host
- `--show-token` print the token after the status block

### `bb auth logout`

```
$ bb auth logout
? You are logged into bitbucket.org as hbackman. Log out? Yes
✓ Logged out of bitbucket.org as hbackman
```

Flags:
- `--hostname <HOST>`
- `--user <USER>`
- `-y, --yes` skip confirmation

Removes the keyring entry and the `hosts.yml` entry. If the `active_user` was removed, prompts to pick another user as active (or unsets if none remain).

### `bb auth token`

Prints the active access token (refreshed if needed) to stdout. Exits with `CliError::Auth` if not logged in. Useful for `curl -H "Authorization: Bearer $(bb auth token)"`.

Flags: `--hostname`, `--user`.

### `bb auth switch`

```
$ bb auth switch
? Switch to which account? [Use arrows]
    hbackman (active)
  > work-account
✓ Switched active account to work-account on bitbucket.org
```

Flags: `--hostname`, `--user`.

### `bb auth setup-git`

Registers `bb` as a git credential helper for the given host. Writes to the *global* `~/.gitconfig`:

```ini
[credential "https://bitbucket.org"]
    helper =
    helper = !/usr/local/bin/bb auth git-credential
```

(First `helper =` line resets any existing helpers; the path comes from `std::env::current_exe()`.)

Flags: `--hostname`, `--force` (rewrite even if present).

### `bb auth git-credential`

The credential helper backend, invoked by git. Implements the [git-credential protocol](https://git-scm.com/docs/git-credential):

```
$ bb auth git-credential get
protocol=https
host=bitbucket.org
```
→
```
protocol=https
host=bitbucket.org
username=x-token-auth
password=<access_token>
```

`store` and `erase` are no-ops (git is just telling us about a credential it cached/dropped — we manage our own storage).

Username `x-token-auth` is Bitbucket's documented "username for token auth over HTTPS."

## File layout

```
src/auth/
├── mod.rs                # AuthSource, public surface
├── oauth.rs              # oauth2 client setup, code exchange, refresh
├── callback.rs           # await_callback HTTP server
├── keyring.rs            # KeyringBackend trait + real `keyring` impl + in-memory mock
├── hosts.rs              # hosts.yml read/write (depends on src/config)
├── user.rs               # fetch_user via GET /2.0/user
├── git_credential.rs     # credential-helper protocol parser/responder
└── tests/                # split out for the bigger pieces (callback, hosts, keyring)

src/cli/auth/
├── mod.rs                # `bb auth ...` clap subcommand tree
├── login.rs
├── logout.rs
├── status.rs
├── token.rs
├── switch.rs
├── setup_git.rs
└── git_credential.rs
```

Each `src/cli/auth/<verb>.rs` exports an `Args` struct and an `async fn run(args, ctx) -> Result<(), CliError>`. The parent `bb auth` subcommand uses a clap `Subcommand` enum to dispatch.

## Browser opener

Use the `webbrowser` crate (cross-platform: `open` on macOS, `xdg-open` on Linux, `rundll32 …` on Windows). It already honors `$BROWSER`.

For tests, inject a trait:

```rust
pub trait Browser: Send + Sync {
    fn open(&self, url: &str) -> Result<()>;
}
```

The default impl wraps `webbrowser::open`; tests use a fake that records the URL.

Wire onto `Context` as `pub browser: OnceCell<Arc<dyn Browser>>` so test contexts can swap it.

## Error handling

- 401 on any auth-related endpoint → `CliError::Auth("you are not logged in. Run `bb auth login`")`.
- 403 on the authorize endpoint when scopes are insufficient → `CliError::Auth(...)` with a message naming the required scope.
- Refresh-token-expired → clear the stored refresh token, return `CliError::Auth` asking the user to re-run login.
- Keyring read/write failure → fall back to plaintext with a warning; never block the operation.

## Required dependencies (deferred from spec 01)

Add to `Cargo.toml` when this slice lands:

```toml
keyring     = "3"
webbrowser  = "1"
time        = { version = "0.3", features = ["serde", "macros", "formatting", "parsing"] }
```

`oauth2 = "4"` is already pinned by spec 01.

## Tests

- OAuth URL building: state token in query, scopes joined with space, redirect_uri matches.
- Callback handler: success path returns code; mismatched state returns error; error param returns error; timeout fires.
- Hosts.yml round-trip preserves unknown keys.
- Keyring round-trip via the in-memory mock backend.
- Git-credential protocol parser handles every documented field.
- `--with-token` path: token is validated via mocked `/2.0/user` and stored as `api_token`.
- Refresh-on-401: API client sees one 401, AuthSource refreshes, second request succeeds. A second consecutive 401 returns `CliError::Auth`.

Use `wiremock` for the Bitbucket endpoint fixtures; spin up `tokio::net::TcpListener` directly for the callback test.

## Concrete deliverables

1. `src/auth/` with the modules listed above, full implementations.
2. `src/cli/auth/` subcommand tree replacing the stub from spec 01.
3. `Context::browser` wiring + `Browser` trait + default impl.
4. `build.rs` additions for embedding OAuth client_id / client_secret.
5. Unit tests as listed above; integration test for the full browser flow against `wiremock`.

## Acceptance criteria

- `bb auth login --client-id X --client-secret Y` against a mocked Bitbucket completes, stores tokens, and prints the username.
- `bb auth login --with-token` (stdin) authenticates with a token.
- `bb auth status` reports the right host/user/scopes after login.
- `bb auth token` prints a token (refreshing if expired).
- `bb auth logout` removes the credential.
- `bb auth setup-git` writes a credential helper line into `~/.gitconfig`, and `git clone https://bitbucket.org/...` succeeds without prompting.
- `bb auth git-credential get` (stdin: `protocol=https\nhost=bitbucket.org\n`) prints `username=x-token-auth\npassword=...`.
- 401 on an API call triggers a single refresh + retry.
- Keyring unavailable → plaintext fallback with warning; nothing crashes.

## Open questions

- Should `bb auth login` default to also running `setup-git` (with a Y/n prompt), or keep them separate? gh prompts. **Lean: prompt by default**, with `--no-setup-git` to skip.
- Do we want `bb auth login --device` for a future device flow if Atlassian adds one? Mention as TODO; do not implement.
- Should `BB_TOKEN` from env appear under a separate user key (e.g. `env`) or shadow the current active user? **Lean: shadow** — simpler.
