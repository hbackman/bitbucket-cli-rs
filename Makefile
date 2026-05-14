.PHONY: build test lint fmt install clean

build:
	cargo build --release

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
