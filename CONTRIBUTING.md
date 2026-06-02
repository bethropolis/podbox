# Contributing

## Building

```bash
cargo build --release                    # host CLI
cargo build --release -p podbox-guest    # static guest binary
```

## Testing

```bash
cargo test                               # unit + integration
cargo clippy -- -D warnings
cargo fmt --check
```

## Pull Requests

- Run `cargo test`, `cargo clippy`, and `cargo fmt --check` before opening.
- `podbox-guest` must compile as a fully static musl binary (`cargo build --release --target x86_64-unknown-linux-musl -p podbox-guest`).
- Keep changes focused — one logical change per PR.
