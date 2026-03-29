all: check build test

export RUSTFLAGS=-Dwarnings

test:
	cargo test

build:
	cargo build
	cargo build --features cli

check:
	cargo check --all-targets
	cargo check --all-targets --features cli

install:
	cargo install --path . --force --features cli

fmt:
	cargo fmt --all

clean:
	cargo clean
