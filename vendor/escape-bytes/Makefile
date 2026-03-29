.PHONY: all test fuzz build doc readme clean fmt

export RUSTFLAGS=-Dwarnings -Dclippy::all -Dclippy::pedantic

all: build test

test:
	cargo hack test --tests --feature-powerset --exclude-features docs
	cargo hack test --doc --all-features
	cargo +nightly fuzz run fuzz -- -runs=0

fuzz:
	cargo +nightly fuzz run fuzz

build:
	cargo hack clippy --feature-powerset --exclude-features docs --all-targets

doc:
	cargo hack test --doc --all-features
	RUSTDOCFLAGS="--cfg doc" cargo +nightly doc --all-features --open

readme:
	cargo readme > README.md

clean:
	cargo clean

fmt:
	cargo fmt --all
