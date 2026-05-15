# bb — Bitbucket Cloud CLI

A Bitbucket Cloud command-line tool modeled on GitHub's `gh`. Authenticate via
browser, manage pull requests and repositories, and call any REST endpoint
through an escape hatch — all from a single static binary.

## Features

- Browser-based OAuth login; tokens stored in the OS keyring (macOS Keychain,
  Linux Secret Service, Windows Credential Manager).
- Pull requests: `list`, `view`, `create`, `checkout`, `diff`, `merge`,
  `close`, `reopen`, `ready`, `edit`, `comment`, `review`, `checks`, `status`.
- Repositories: `list`, `view`, `clone`, `create`, `fork`, `set-default`.
- `bb api` — call any Bitbucket REST endpoint directly when a higher-level
  command isn't enough.
- Structured output with `--json <fields>` and inline `--jq <filter>` for
  pipelines.
- Shell completions for bash, zsh, fish, and PowerShell via `bb completion`.

## Install

### macOS / Linux (Homebrew)

```sh
brew install hbackman/bb/bb
```

### Manually

Download the latest release for your platform from
<https://github.com/hbackman/bitbucket-cli/releases/latest> and extract the
`bb` binary into a directory on your `$PATH`.

On macOS, if Gatekeeper blocks the binary after a manual download, clear the
quarantine attribute:

```sh
xattr -d com.apple.quarantine /usr/local/bin/bb
```

### From source

```sh
cargo install --git https://github.com/hbackman/bitbucket-cli bb
```

## Quick start

```sh
# One-time: sign in via your browser. Tokens land in the OS keyring.
bb auth login

# Inside a Bitbucket clone:
bb pr list                       # PRs on the current repo
bb pr view 42                    # one PR (use `bb pr view` for the current branch)
bb pr create --fill --draft      # open a draft PR using the latest commit's
                                 # subject/body for title/body
bb pr checks 42                  # pipeline status for a PR
bb repo view                     # repository details

# JSON output and inline filtering:
bb pr list --json id,title,author --jq '.[] | "\(.id) \(.title)"'

# Generic REST escape hatch:
bb api repositories/{workspace}/{repo}/pullrequests/42
```

Run `bb help` or `bb <command> --help` for the full command surface.

## Building from a checkout

```sh
cargo build --release
./target/release/bb --version
```

## Specs

Design lives in [`docs/specs/`](docs/specs/). Start with
[`00-overview.md`](docs/specs/00-overview.md).

## License

MIT — see [LICENSE](LICENSE).
