# bbk — Bitbucket Cloud CLI

A Bitbucket Cloud command-line tool modeled on GitHub's `gh`. Authenticate via
browser, manage pull requests and repositories, and call any REST endpoint
through an escape hatch — all from a single static binary.

## Features

- Browser-based OAuth login; tokens stored in the OS keyring (macOS Keychain,
  Linux Secret Service, Windows Credential Manager).
- Pull requests: `list`, `view`, `create`, `checkout`, `diff`, `merge`,
  `close`, `reopen`, `ready`, `edit`, `comment`, `review`, `checks`, `status`.
- Repositories: `list`, `view`, `clone`, `create`, `fork`, `set-default`.
- `bbk api` — call any Bitbucket REST endpoint directly when a higher-level
  command isn't enough.
- Structured output with `--json <fields>` and inline `--jq <filter>` for
  pipelines.
- Shell completions for bash, zsh, fish, and PowerShell via `bbk completion`.

## Install

### macOS / Linux

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/hbackman/bitbucket-cli-rs/releases/latest/download/bbk-installer.sh | sh
```

The installer detects your OS and architecture (Intel/Apple Silicon on macOS;
x86_64/aarch64 on Linux), downloads the matching tarball from the latest
GitHub release, and places `bbk` under `$CARGO_HOME/bin` (or `~/.cargo/bin`).

### Windows (PowerShell)

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/hbackman/bitbucket-cli-rs/releases/latest/download/bbk-installer.ps1 | iex"
```

### Manually

Download the tarball for your platform from
<https://github.com/hbackman/bitbucket-cli-rs/releases/latest>, extract it,
and place `bbk` somewhere on your `$PATH`.

On macOS, if Gatekeeper blocks the binary after a manual download, clear the
quarantine attribute:

```sh
xattr -d com.apple.quarantine /usr/local/bin/bbk
```

### From source

```sh
cargo install --git https://github.com/hbackman/bitbucket-cli-rs bbk
```

## Quick start

```sh
# One-time: sign in via your browser. Tokens land in the OS keyring.
bbk auth login

# Inside a Bitbucket clone:
bbk pr list                       # PRs on the current repo
bbk pr view 42                    # one PR (use `bbk pr view` for the current branch)
bbk pr create --fill --draft      # open a draft PR using the latest commit's
                                  # subject/body for title/body
bbk pr checks 42                  # pipeline status for a PR
bbk repo view                     # repository details

# JSON output and inline filtering:
bbk pr list --json id,title,author --jq '.[] | "\(.id) \(.title)"'

# Generic REST escape hatch:
bbk api repositories/{workspace}/{repo}/pullrequests/42
```

Run `bbk help` or `bbk <command> --help` for the full command surface.

## Building from a checkout

```sh
cargo build --release
./target/release/bbk --version
```

## Specs

Design lives in [`docs/specs/`](docs/specs/). Start with
[`00-overview.md`](docs/specs/00-overview.md).

## License

MIT — see [LICENSE](LICENSE).
