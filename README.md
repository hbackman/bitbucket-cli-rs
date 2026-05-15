# bb — Bitbucket Cloud CLI

A Bitbucket Cloud command-line tool modeled on GitHub's `gh`. Authenticate via
browser, manage pull requests and repositories, and call any REST endpoint
through an escape hatch — all from a single static binary.

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
