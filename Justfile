# ──────────────────────────────────────────────
#  podbox — development justfile
# ──────────────────────────────────────────────

alias c := check
alias b := build
alias t := test
alias l := lint

# Run all checks (lint + test)
check: lint test

# Lint
lint:
    cargo clippy --all-targets -- -D warnings

# Run all tests
test:
    cargo test --all-targets

# Build everything
build:
    cargo build --all-targets

# Build release
release:
    cargo build --release --all-targets
    cargo build --release -p podbox-guest --target x86_64-unknown-linux-musl

# Build guest binary (statically linked)
guest:
    cargo build --release -p podbox-guest --target x86_64-unknown-linux-musl

# Full rebuild cycle: check → build guest → build host
cycle: check guest release
    @echo "---"
    @echo "Release artifacts:"
    @ls -lh target/release/podbox
    @ls -lh target/x86_64-unknown-linux-musl/release/podbox-guest
