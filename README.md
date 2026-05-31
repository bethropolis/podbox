# podmgr

A Podman-native Rust workspace that turns a single TOML definition file into a
fully integrated, systemd-managed container environment. Two binaries: `podmgr`
(host CLI) and `podmgr-guest` (static musl sidecar baked into every built image).
Together they provide selective XDG directory sharing, Wayland/audio passthrough,
a bidirectional host-guest socket protocol, and `.desktop`/bin shim export -- all
without a persistent daemon and without mounting the host home directory wholesale.

## Quick Start

```bash
# 1. Install
git clone <repo-url> && cd podmgr
cargo install --path crates/podmgr

# 2. Build the guest binary (required before building images)
rustup target add x86_64-unknown-linux-musl
cargo build -p podmgr-guest --release --target x86_64-unknown-linux-musl

# 3. Create a definition file
cat > myenv.toml << 'EOF'
[image]
base = "fedora:41"
name = "myenv"

[image.packages]
install = ["git", "gcc", "firefox"]

[container]
name  = "myenv"
home  = "~/containers/myenv"

[integration]
wayland = true
audio   = true
notify  = true
xdg_open = true

[lifecycle]
quadlet   = true
autostart = true
EOF

# 4. Build and enable
podmgr --config myenv.toml build
podmgr --config myenv.toml enable

# 5. Start and enter
podmgr --config myenv.toml shell
# inside: notify-send "hello"    # → appears on host
# inside: xdg-open https://...   # → opens in host browser
```

## Definition File Format

```toml
[image]
base = "fedora:41"          # base OCI image
name = "myenv"              # image tag name

[image.packages]
install = ["git", "gcc"]
remove  = []

[image.run]
commands = ["dnf clean all"]

[container]
name  = "myenv"
home  = "~/containers/myenv"    # isolated container home
shell = "bash"

[container.mounts]
extra = ["~/Work:/home/user/Work:z"]

[container.env]
EDITOR = "nvim"

[integration]
wayland     = true    # Wayland socket passthrough
audio       = true    # PipeWire/PulseAudio passthrough
gpu         = "auto"  # "auto"|true|false|"nvidia" — GPU passthrough
dbus        = true    # D-Bus session passthrough
notify      = true    # notify-send → host notifications
xdg_open    = true    # xdg-open → host browser
clipboard   = true    # wl-copy/wl-paste bridge
sync_themes = true    # mount ~/.themes :ro
sync_icons  = true    # mount ~/.icons :ro
sync_fonts  = true    # mount ~/.fonts + fontconfig :ro

[integration.xdg_dirs]
documents = true    # mount ~/Documents into container
downloads = true
pictures  = false   # opt-in only

[integration.export]
apps = ["gedit"]    # export .desktop to host
bins = ["rg"]       # export bin shim to ~/.local/bin

[systemd]
requires = ["db-container.service"]  # systemd Requires= dependencies
after    = ["network.target"]         # systemd After= dependencies

[lifecycle]
quadlet     = true    # generate systemd Quadlet files
autostart   = true    # start on login
on_stop     = "keep"  # "keep" or "remove"
auto_update = false   # add auto-update label (io.containers.autoupdate=registry)
```

## Commands

| Command | Description |
|---------|-------------|
| `podmgr build` | Generate Containerfile and build image |
| `podmgr enable` | Install Quadlet systemd files |
| `podmgr disable` | Remove Quadlet files |
| `podmgr start` | Start the container |
| `podmgr stop` | Stop the container |
| `podmgr shell` | Open interactive shell (uses TTY) |
| `podmgr exec -- <cmd>` | Execute command interactively |
| `podmgr run <app>` | Run GUI app detached |
| `podmgr status` | Show container state |
| `podmgr logs [-f]` | Show container logs |
| `podmgr export app <name>` | Export .desktop file to host |
| `podmgr export bin <name>` | Export bin shim to host |
| `podmgr remove [--all]` | Remove container |
| `podmgr doctor` | Run diagnostic checks |
| `podmgr translate-path --to-container <path>` | Translate host path to container path |
| `podmgr translate-path --to-host <path>` | Translate container path to host path |
| `podmgr completions <shell>` | Generate shell completions |

All commands support `--dry-run` to preview actions without executing.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error (runtime, I/O, etc.) |
| 2 | Configuration error (file not found, parse failure) |
| 3 | Container missing |
| 4 | Build or podman inspect failure |
| 5 | Missing dependency (podman, podmgr-guest not found) |

## Architecture

```
myenv.toml → [podmgr build] → Containerfile → podman build → image
           → [podmgr enable] → .build + .socket + .container → systemd

Container startup:
  catatonit (PID 1)
    → podmgr-entry.sh
      → fork() → podmgr-guest --daemon (connects to host socket)
      → exec() → bash (user shell)
```

The `podmgr-guest` daemon inside the container:
- Connects to a host Unix socket for bidirectional communication
- Installs symlinks (`notify-send`, `xdg-open`) in `/run/podmgr/bin/`
- Injects PATH via `/etc/environment.d/podmgr.conf`
- Forwards notifications, xdg-open requests, and clipboard operations to the host

## GPU Passthrough

`podmgr` supports automatic GPU detection. The `gpu` field accepts:

| Value | Behavior |
|-------|----------|
| `"auto"` (default) | Detects `/dev/dri` (Intel/AMD) and/or `/dev/nvidia*` at container start |
| `true` | Enables `/dev/dri` for all GPU types |
| `false` | No GPU passthrough |
| `"nvidia"` | Enables DRI + NVIDIA device nodes (nvidiactl, nvidia0, nvidia-uvm) |

## Systemd Dependencies

Custom `Requires=` and `After=` directives in the generated Quadlet `[Unit]` section:

```toml
[systemd]
requires = ["db-container.service"]
after    = ["network.target", "db-container.service"]
```

## Selective XDG Directory Sharing

Instead of mounting the entire host home, podmgr lets you opt-in to specific
XDG user directories:

```toml
[integration.xdg_dirs]
documents = true   # ~/Documents → /root/Documents
downloads = true   # ~/Downloads → /root/Downloads
pictures  = false  # not mounted
```

## Requirements

- Podman >= 4.6 (Quadlet support)
- systemd (user session)
- Linux with Wayland (for GUI passthrough)
- xdg-user-dirs (for XDG directory resolution)
- Rust toolchain (to build)

## License

MIT
