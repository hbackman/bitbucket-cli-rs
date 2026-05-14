# bb — Bitbucket Cloud CLI

A Bitbucket Cloud command-line tool modeled on GitHub's `gh`. Authenticate via
browser, manage pull requests and repositories, and call any REST endpoint
through an escape hatch — all from a single static binary.

> **Status:** Pre-alpha. Only the scaffolding is in place; commands print
> "not yet implemented". See `docs/specs/` for the planned surface.

## Building

```sh
cargo build --release
./target/release/bb --version
```

## Specs

Design lives in [`docs/specs/`](docs/specs/). Start with
[`00-overview.md`](docs/specs/00-overview.md).

## License

MIT — see [LICENSE](LICENSE).
