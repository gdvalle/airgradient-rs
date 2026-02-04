default: check fmt lint

check:
    cargo check --bin airgradient

fmt:
    cargo fmt

lint:
    cargo clippy

fix:
    cargo fmt
    cargo clippy --fix

run:
    cargo run --release

unused-deps:
    cargo +nightly udeps
