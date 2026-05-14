.PHONY: build test lint fmt install clean dev run

# Auto-source .env if present so OAuth creds (BB_OAUTH_CLIENT_ID/_SECRET) are
# available to build.rs and bb at runtime. .env is gitignored.
ifneq (,$(wildcard .env))
    include .env
    export
endif

build:
	cargo build --release

dev:
	cargo build

run:
	cargo run --quiet -- $(ARGS)

test:
	cargo test --all

lint:
	cargo clippy --all-targets --all-features -- -D warnings
	cargo fmt --all -- --check

fmt:
	cargo fmt --all

install:
	cargo install --path .

clean:
	cargo clean
