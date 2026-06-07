# Contributing

## Project Structure

```
crates/
  podbox/           # Host CLI — commands, config, codegen, diff, socket host
  podbox-guest/     # Guest daemon — embedded in host binary at build time
scripts/            # Install, uninstall, bump-version, logo generation
```

## Building

```bash
cargo build --release -p podbox          # host CLI (embeds guest automatically)
```

The guest binary (`podbox-guest`) is compiled as a static musl binary by
`build.rs` during the host build and embedded via `include_bytes!`. No
separate build step needed.

## Testing

```bash
cargo test                               # all unit + integration tests
cargo clippy -- -D warnings
cargo fmt --check
```

## Pull Requests

- Run `cargo test`, `cargo clippy`, and `cargo fmt --check` before opening.
- Keep changes focused — one logical change per PR.

## Architecture Support

Targets are validated in CI on every push:
- **x86_64** (`x86_64-unknown-linux-gnu` + `x86_64-unknown-linux-musl`)
- **aarch64** (`aarch64-unknown-linux-gnu` + `aarch64-unknown-linux-musl`)

Cross-compilation for arm64 requires `gcc-aarch64-linux-gnu` on the host.

## Pre-commit Hook

```bash
git config core.hooksPath .githooks
```

The pre-push hook warns if the Cargo.toml version doesn't match the latest
tag — useful for catching version bump omissions before CI.

## Versioning

```bash
scripts/bump-version.sh <new-version>    # updates Cargo.toml, commits, tags
```

Tags follow the `v<major>.<minor>.<patch>` scheme (e.g. `v0.3.8`).

## Release Pipeline

Pushing a tag triggers `.github/workflows/release.yml`:
1. Build host binaries for x86_64 + aarch64 (gnu + musl)
2. Build Docker images for Fedora (multi-arch), CachyOS, and Dev (amd64)
3. Publish archives, AUR package, Homebrew cask
4. Generate checksums
