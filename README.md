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

# 3. Initialise a profile (creates ~/.config/podmgr/cachy.toml)
podmgr init cachy

# 4. Create, build, enable, and start in one command
podmgr create cachy

# 5. Enter the container
podmgr shell
```

Prebuilt profiles: `cachy` (Arch-based CachyOS), `fedora` (Fedora 44).
You can also pass a full image reference instead of a profile name:

```bash
podmgr create ghcr.io/username/my-image:latest
```

## Definition File Format

```toml
[image]
base = "fedora:44"          # base OCI image
name = "myenv"              # image tag name

[image.packages]
install = ["git", "gcc"]
remove  = []

[image.run]
commands = ["dnf clean all"]

[container]
name  = "myenv"
home  = "~/containers/myenv"    # isolated container home
shell = "/usr/bin/fish"

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
| `podmgr init [profile]` | Scaffold a config file from a built-in profile or template |
| `podmgr create [profile\|image]` | Init → build → enable → start in one command |
| `podmgr list` | List podmgr-managed containers |
| `podmgr build` | Generate Containerfile and build image |
| `podmgr enable` | Install Quadlet systemd files |
| `podmgr disable` | Remove Quadlet files |
| `podmgr start` | Start the container (auto-heals missing image/Quadlet) |
| `podmgr stop` | Stop the container |
| `podmgr shell` | Open interactive shell (default: fish) |
| `podmgr enter <name>` | Enter a named container (auto-starts if stopped) |
| `podmgr exec -- <cmd>` | Execute command interactively |
| `podmgr run <app>` | Run GUI app detached |
| `podmgr status` | Show container state |
| `podmgr refresh` | Rebuild image and regenerate Quadlet from current config |
| `podmgr logs [-f]` | Show container logs |
| `podmgr serve [--port <port>]` | Start the host socket server (daemon for bidirectional protocol) |
| `podmgr export app <name>` | Export .desktop file to host |
| `podmgr export bin <name>` | Export bin shim to host |
| `podmgr export clean [--all]` | Remove stale exported shims |
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
           → [podmgr enable] → .container + .socket → systemd

Container startup:
  podmgr-guest --entry (PID 1)
    → fork() → podmgr-guest --daemon (connects to host socket)
    → exec() → /usr/bin/fish (user shell, or SHELL from config)
```

The `podmgr-guest` daemon inside the container:
- Connects to a host Unix socket for bidirectional communication
- Installs symlinks (`notify-send`, `xdg-open`) in `/run/podmgr/bin/`
- Injects PATH via `/etc/environment.d/podmgr.conf`
- Forwards notifications, xdg-open requests, and clipboard operations to the host

At container start `podmgr-guest --entry` (running as root inside, which maps
to host UID 1 via `UserNS=keep-id`) creates a matching system user, grants
passwordless sudo, and makes the home directory writable. The default shell
is fish (4.7+). On `podman exec -u bet`, the user enters with UID 1000 (host
root) which has full access to Wayland/dconf sockets and the bind-mounted home.

## Idmapped Mounts & UID Shift

`UserNS=keep-id` creates an idmapped mount that shifts filesystem UIDs by 1
inside the container (host UID 1000 → container UID 999). The entrypoint
handles this automatically:

1. Reads the actual owner of the bind-mounted home directory
2. Keeps the `bet` user at its original UID (1000)
3. Makes the home world-writable (`chmod 777`) so fish history, config, and
   universal variables work regardless of the UID shift

No `chown` is performed on bind-mounted directories — doing so would corrupt
host ownership through the idmapped mount.

## Prebuilt Images

The GoReleaser pipeline (`dockers_v2`) builds and pushes prebuilt images to
`ghcr.io/bethropolis/podmgr-images` on every `v*` tag. Profiles with
`prebuilt = true` pull these instead of building from scratch.

Available images:
- `cachy-latest` — CachyOS (Arch-based), fish 4.7, podman-guest sidecar
- `fedora-latest` — Fedora 44, fish 4.7, podman-guest sidecar

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
documents = true   # ~/Documents → /home/bet/Documents
downloads = true   # ~/Downloads → /home/bet/Downloads
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
