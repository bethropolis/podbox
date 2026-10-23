# AGENTS.md

Guidance for coding agents working in this repository. Humans: see
`CONTRIBUTING.md` and `docs/`.

## What podbox is

A declarative, Podman-native container environment manager. One TOML
definition per environment is the single source of truth; podbox generates the
Containerfile, Quadlet units, lock files and desktop entries from it, and
systemd owns the lifecycle. There is no host daemon: a small guest binary runs
inside each container and talks to the host CLI over a Unix socket.

Read `docs/architecture.md` before changing anything structural.

## Layout

```
crates/
  podbox/            package `podbox-cli`, lib `podbox`, bin `podbox`: host CLI,
                     config, codegen, socket host, systemd/Quadlet install
  podbox-guest/      guest daemon + interceptors (notify-send, xdg-open,
                     clipboard, host-exec); built as static musl and embedded
                     into the host binary by crates/podbox/build.rs
  podbox-protocol/   wire format shared by host and guest (frames, messages,
                     capability constants, PROTOCOL_VERSION)
  podbox-wasm/       podbox config/codegen compiled to wasm for the website Studio
docs/                user documentation, the source of truth for the website docs
images/              base image Containerfiles (fedora, cachy, ubuntu, dev)
scripts/             install/uninstall, version bump, wasm build, asset generators
website/             Astro + React + Tailwind site (bun); has its own CLAUDE.md
```

Inside `crates/podbox/src`: `config/` (schema, defaults, validation, file
lookup), `codegen/` (Containerfile + `quadlet/`), `commands/` (one module per
CLI verb), `socket_host/` (per-connection handling, handlers), `main/`
(dispatch, config resolution), `profiles/` (built-in profile TOMLs).

## Commands

The Rust toolchain is pinned in `rust-toolchain.toml`; other tools in
`mise.toml`; `mise install` is the recommended setup (see CONTRIBUTING.md). Some dev hosts have no Rust toolchain; build
inside a podbox or toolbox dev container in that case.

```bash
just check                       # clippy -D warnings + cargo test --all-targets
cargo fmt --check
cargo test -p podbox-cli --test <name>        # one integration test file
cargo check -p podbox-cli --lib --no-default-features --target wasm32-unknown-unknown
cargo build --release -p podbox-cli           # host CLI, guest embedded
```

CI (`.github/workflows/ci.yml`) runs fmt, clippy (`--workspace --all-targets
-D warnings`), `cargo test --workspace`, the wasm32 check above, and in
`website/`: `bun run typecheck` and `bun run check:studio`. Run whatever your
change touches before calling it done.

Website (`cd website`): `bun run dev`, `bun run typecheck`,
`bun run check:studio`, `bun run build`. `check:studio` needs the wasm
bindings, rebuilt with `scripts/build-wasm.sh` whenever the config schema or
codegen changes (the output in `website/src/wasm` is committed).

## Rules

### Rust
- `unsafe_code = "deny"` workspace-wide. A genuinely needed block gets a scoped
  `#[allow(unsafe_code)]` and a `// SAFETY:` comment (clippy enforces the latter).
- Clippy `all` + `pedantic` at warn, and CI denies warnings. Do not add blanket
  `allow`s; fix the lint or allow it at the narrowest scope with a reason.
- Edition 2024. Follow the surrounding style: doc comments on public items,
  comments that explain *why*, `anyhow` in commands, typed `PodboxError`
  where a caller or exit code depends on the variant.

### Architecture
- **Codegen is pure**: config in, string out. No filesystem, environment or
  podman calls inside `codegen/`. Host facts (does a device exist, which
  podman version) are gathered in orchestration and passed in.
- **The `podbox` lib must build for wasm32 without the `cli` feature.** Keep
  OS-specific code (nix, process spawning, sockets) behind `cli` so the Studio
  keeps working.
- **Protocol changes go in `podbox-protocol`** and must stay compatible with
  guests already baked into users' images. Additive message types do not bump
  `PROTOCOL_VERSION`; breaking changes do. Use the `CAP_*` constants, never
  inline capability strings. Every guest connection sends `Hello` and checks
  the granted capability before any request.
- **Names**: `container.name` is the runtime name podman and systemd see; the
  config file name may differ. Use `Config::runtime_name()`,
  `image_repo()` and `local_image_tag()` rather than formatting
  `localhost/podbox-…` by hand. Validate any user-supplied name with
  `config::validate_container_name` before it reaches a path or argv.
- **Shelling out**: use the helpers in `process.rs`. Mutating calls use the
  checked variants (`run_checked`, `spawn_interactive_checked`) so a non-zero
  exit is an error, not silent success.
- **`--dry-run`** must not write files or run a state-changing podman/systemctl
  verb. New mutating commands print `would: …` instead.

### CLI contract
`docs/clients.md` defines what external clients may rely on: JSON payloads
carry `"schema": 1`, errors have stable `code` strings, and exit codes are
listed in `docs/cli.md`. Treat these as API:
- Do not rename or remove JSON fields or error codes; additions are fine.
- A new exit code or error code needs the docs tables updated in the same change.
- `--output json` commands print nothing but JSON on stdout.
- Golden fixtures under `crates/podbox/tests/fixtures/golden` pin the shapes;
  update them deliberately, never just to make a test pass.

### Config schema
A new config key touches, in one change:
1. the type in `crates/podbox/src/config/types*.rs` (serde default +
   `skip_serializing_if` so defaults stay out of written TOML), and validation;
2. codegen and any `doctor` check;
3. `docs/config.md`;
4. the Studio: `website/src/design/islands/studio/schema.ts` field table,
   `useStudioState.ts`, the relevant panel, and a case in
   `website/scripts/check-studio-parity.ts`;
5. rebuilt wasm bindings.

### Tests
- Integration tests live in `crates/podbox/tests/`, with fixtures in
  `tests/fixtures/` and `insta` snapshots in `tests/snapshots/`. Review
  snapshot changes with `cargo insta review`; don't blindly accept them.
- Tests must not need podman, systemd or a running session. Stub external
  binaries (see `tests/sandbox/`) and use temp dirs. Never touch the real
  `~/.config/podbox` or the user's containers.
- A bug fix comes with a test that fails without it.

### Docs and website
- `docs/*.md` is the source. `website/src/content/docs/`, the website's copies
  of `docs/assets`, and `website/src/design/searchIndex.ts` are generated by
  `website/scripts/sync-content.ts`; never edit them by hand.
- Diagrams and brand assets are generated: `scripts/gen-diagrams.py` and
  `scripts/gen-brand.py` write `docs/assets/`. Change the script, then
  re-run it.
- Website-specific conventions are in `website/CLAUDE.md`.

### Licensing
The project is MIT. Do not copy code from GPL projects (distrobox included).
Re-implement from a description. toolbox (Apache-2.0) and podlet (MPL-2.0)
can inform designs; credit them if you follow one closely.

## Git
- One logical change per commit, with a short imperative subject and the
  reasoning in the body (see `git log`).
- Don't bump the version by hand; `scripts/bump-version.sh` does it and tags.
- Don't commit `target/`, `website/dist`, or an unrelated `website/bun.lock`
  churn.
- `hk check` (`hk.pkl`) runs the repo linters: cargo fmt, shellcheck,
  whitespace, merge markers, large files, private keys. Its pre-push hook runs
  clippy and warns if `Cargo.toml` drifts from the latest tag.
