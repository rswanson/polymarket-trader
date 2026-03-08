.PHONY: build release check clippy fmt lint clean install

build:
	cargo build

release:
	cargo build --release

check:
	cargo check

clippy:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

lint: clippy fmt-check

clean:
	cargo clean

install:
	cargo install --path .
