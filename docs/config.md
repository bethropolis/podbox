---
description: Complete TOML configuration reference for podbox — all keys, defaults, and examples for image, container, integration, lifecycle, and D-Bus settings.
---

# Configuration Reference

`podbox` searches for a definition file in this order:

1. `./.podbox.toml` (project-local)
2. Active context from `~/.config/podbox/.active` (set via `podbox use <name>`)
3. `~/.config/podbox/*.toml` (first file, sorted by name)
4. Embedded default (`fedora:44`, name `podbox`)

## `[image]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `base` | string | *required* | Base container image (e.g. `"fedora:44"`) |
| `name` | string | *required* | Image tag name (e.g. `"myenv"`) |
| `image` | string | — | Prebuilt image reference (e.g. `"ghcr.io/user/myenv:latest"`). When set, podbox uses the registry image instead of building from `base` |
| `pull_retry` | int | `3` | Number of pull retries on failure |
| `pull_retry_delay` | string | `"5s"` | Delay between pull retries |

### `[image.packages]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `install` | string[] | `[]` | Packages to install via the distro package manager |
| `remove` | string[] | `[]` | Packages to remove |
| `manager` | string | auto-detected | Package manager override: `dnf`, `apt`, `pacman`, `apk`, `zypper`. Auto-detected from the base image name when omitted |

### `[image.run]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `commands` | string[] | `[]` | Extra `RUN` commands in the Containerfile |

```toml
[image]
base = "fedora:44"
name = "myenv"

[image.packages]
install = ["git", "gcc", "ripgrep"]
remove = ["vim-minimal"]

[image.run]
commands = ["dnf clean all"]
```

## `[container]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `name` | string | *required* | Container name (used for systemd unit names, socket paths) |
| `home` | string | *required* | Host path for isolated home (`~` expands) |
| `shell` | string | `"fish"` | Default login shell inside the container |
| `memory` | string | — | Memory limit (e.g. `"4G"`, `"2048M"`). Passed as `Memory=` in Quadlet |
| `cpus` | string | — | CPU limit (e.g. `"2.0"`, `"0.5"`). Passed through to Podman as `--cpus` |
| `slice` | string | `"podbox.slice"` | systemd slice for the container service |
| `cpu_weight` | integer | `200` | systemd CPU contention weight (1–10000) |
| `reload_cmd` | string | — | Command run on config reload. Passed as `ReloadCmd=` in Quadlet |

### `[container.mounts]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `extra` | string[] | `[]` | Extra `Volume=` lines (e.g. `"~/Work:/home/user/Work:z"`) |

### `[container.env]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `*` | string | — | Arbitrary environment variables passed to the container |
| `forward` | string[] | `[]` | Host variables to forward for `enter`, `exec`, and `run`; supports exact names and `PREFIX_*` patterns |

```toml
[container]
name = "myenv"
home = "~/containers/myenv"
shell = "zsh"

[container.mounts]
extra = ["~/Projects:/home/user/Projects:z"]

[container.env]
EDITOR = "nvim"
TERM = "xterm-256color"
forward = ["SSH_AUTH_SOCK", "AWS_*"]

[container.services]
redis = "redis-server /etc/redis/redis.conf"
postgres = { command = "postgres -D /home/user/pgdata", restart = "on-failure" }
```

Services run under the guest daemon (the container's init stays Podman's).
Short form restarts on failure; long form takes `restart = "never"`,
`"on-failure"`, or `"always"`, plus an `env` table. Logs go to
`/run/podbox/services/<name>.log`. Services don't block idle shutdown.

## `[dotfiles]`

One-time dotfiles bootstrap during `podbox create`. Host sources copy into
the container home; Git sources clone on the host, so your SSH keys stay
where they are.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `source` | string | *required* | `host:<path>` to copy a local directory, or a Git URL/reference to clone |
| `target` | string | `~/.dotfiles` | Destination inside the container home; must stay within that home |
| `clone_on` | string | `"host"` | Git clone location: `"host"` or `"container"` |
| `install` | string | — | Shell command run inside the container from `target` after acquisition |

```toml
[dotfiles]
source = "host:~/.dotfiles"
target = "~/.dotfiles"
install = "./install.sh"
```

Runs once during `create` — never on plain starts. If creation used
`--no-start` or failed halfway: `podbox dotfiles sync [name]` retries,
`podbox dotfiles status [name]` shows state. Failures are warnings; the
container still works.

<details>
<summary>Environment available to the install command</summary>

`PODBOX=1`, `PODBOX_CONTAINER`, `PODBOX_DISTRO`, `PODBOX_HOME`,
`PODBOX_DOTFILES_DIR`. `PODBOX_PROFILE` is set when the image name matches
a built-in profile.

</details>

## `[security]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `apparmor` | string | — | AppArmor profile name. Passed as `AppArmor=` in Quadlet (`"unconfined"` to disable) |
| `seccomp` | string | — | Seccomp profile path, `"default"`, or `"unconfined"`. Passed as `SeccompProfile=` |
| `security_label_disable` | bool | `true` | Disable SELinux process labeling. Emits `SecurityLabelDisable=true` when set |
| `no_new_privileges` | bool | `true` | Block privilege escalation via setuid binaries (`sudo`, `su`, AUR helpers). Emits `NoNewPrivileges=true` in the Quadlet. Set `false` to allow. |
| `read_only_rootfs` | bool | `false` | Make root filesystem read-only. Emits `ReadOnly=true` in Quadlet |
| `userns` | string | — | User namespace mode override. Defaults to `"keep-id"`. Supported: `"keep-id"`, `"nomap"`, `"private"` |
| `cap_preset` | string | `"default"` | Capability preset. Options: `"none"`, `"default"`, `"monitoring"`, `"admin"`. Adds a predefined set of `--cap-add` entries alongside any `cap_add` list below |
| `cap_add` | string[] | `[]` | Extra Linux capabilities to add (e.g. `["SYS_ADMIN"]`). Combined with `cap_preset` caps |
| `secrets` | table[] | `[]` | Secrets passed to the container without baking them into the image (see below) |

### `[security].secrets`

Values the container needs but the image must never contain. Short form
reads a `podman secret` and exposes it as a same-named variable:

```toml
[security]
secrets = ["openai_key"]              # Secret=openai_key,type=env,target=openai_key
```

Use the long form for a different target, a file instead of a variable,
a mode, or a systemd credential source:

```toml
[[security.secrets]]
name = "aws_creds"
type = "mount"                        # env (default) or mount
target = "/run/secrets/aws"           # destination name inside the container
mode = "0400"

[[security.secrets]]
name = "gh_token"
source = "systemd"                    # podman (default) or systemd
target = "GH_TOKEN"                   # Environment=GH_TOKEN=%d/gh_token
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `name` | string | *required* | Secret name to read |
| `type` | string | `"env"` | `env` (environment variable) or `mount` (file in the container) |
| `target` | string | secret name | Destination name inside the container |
| `mode` | string | — | File mode for `mount` secrets |
| `source` | string | `"podman"` | `podman` reads `podman secret`; `systemd` reads a systemd credential |

Don't mix forms: all bare strings, or all tables. `podbox doctor` checks
the named secrets exist.

```toml
[security]
apparmor = "unconfined"
seccomp = "default"
read_only_rootfs = true
userns = "nomap"
cap_preset = "monitoring"
cap_add = ["SYS_ADMIN"]
```

## `[network]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `mode` | string | `"pasta"` | Network mode: `"host"`, `"bridge"`, `"none"`, `"pasta"`, `"slirp4netns"`, `"private"`. Defaults to `"pasta"` (user-space NAT with working networking). `"private"` is loopback only — no host sockets or localhost services. `"host"` shares the host network — choose it deliberately |
| `ports` | string[] | `[]` | Port mappings (`"hostPort:containerPort"`). Emitted as `PublishPort=` in Quadlet (ignored in `host` mode) |

```toml
[network]
mode = "pasta"
ports = ["8080:80", "443:443"]
offline = false
```

`offline = true` forces `Network=none` while keeping your `mode` in the
definition. Container-wide only — no per-command override.

## `[storage.shared_caches]`

Opt-in caches shared between podbox containers. Removing a container keeps
its volumes.

```toml
[storage.shared_caches]
cargo = true
mbx = true
rustup = false

[[storage.shared_caches.custom]]
name = "models"
container_path = "~/.cache/models"
```

| Key | Type | Default | Path shared |
|-----|------|---------|-------------|
| `cargo` | bool | `false` | `~/.cargo/registry`, `~/.cargo/git` |
| `npm` | bool | `false` | `~/.npm` |
| `pnpm` | bool | `false` | `~/.local/share/pnpm/store` |
| `pip` | bool | `false` | `~/.cache/pip` |
| `uv` | bool | `false` | `~/.cache/uv` |
| `yarn` | bool | `false` | `~/.cache/yarn`, `~/.yarn/berry/cache` |
| `bun` | bool | `false` | `~/.bun/install/cache` |
| `composer` | bool | `false` | `~/.cache/composer` |
| `maven` | bool | `false` | `~/.m2/repository` |
| `gradle` | bool | `false` | `~/.gradle/caches` |
| `ccache` | bool | `false` | `~/.cache/ccache` |
| `go` | bool | `false` | `~/go/pkg/mod` |
| `rustup` | bool | `false` | `~/.rustup` |
| `mbx` | bool | `false` | `~/.cache/mbx` |
| `custom[].name` | string | — | Built-in names are reserved |
| `custom[].container_path` | string | — | Destination in the container, `~/…` or absolute |

`cargo` shares only registry + git — `~/.cargo/bin` holds binaries and stays
per-container. `podbox cache list` shows volumes, `podbox cache prune NAME`
removes one.

These volumes serve containers only. To reuse a cache you already keep on
the host, use `[storage.host_caches]` below.

## `[storage.host_caches]`

Same toggles, different source: a directory you already keep on the host,
bind-mounted into the container. Use this when the host owns the cache;
use `shared_caches` when only containers use it.

Same built-ins as `shared_caches` — same relative path on both sides.

```toml
[storage.host_caches]
npm = true
mbx = true

[[storage.host_caches.custom]]
name = "zig"
host_path = "~/.cache/zig"
container_path = "~/.cache/zig"
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `cargo` | bool | `false` | `~/.cargo/registry` and `~/.cargo/git` |
| `npm`, `pnpm`, `pip`, `uv`, `yarn`, `bun`, `composer`, `maven`, `gradle`, `ccache`, `go`, `rustup`, `mbx` | bool | `false` | That tool's cache; see the table above for paths |
| `custom[].name` | string | — | Label used in error messages; built-in names are reserved |
| `custom[].host_path` | string | — | Path on the host, `~/…` or absolute |
| `custom[].container_path` | string | — | Destination in the container, `~/…` or absolute |

`custom` mounts any host dir at any container path:

Mounts stay correct when host and container usernames differ (`%h` on the
host side, `/home/%u` on the container side). Unlike `shared_caches` there's
no `:U` remap — the host dir is already yours, and `keep-id` keeps it that
way inside.

<details>
<summary>Three things that bite</summary>

- One directory, two writers: a container can evict host cache entries, and
  any size budget applies to both.
- Share caches, not build state. Two simultaneous builds of the same
  workspace (cargo, mbx, …) collide on target dirs — serialise them.
- A path already in `[container.mounts].extra` is refused. Two mounts for
  one destination would fail inside Podman with an opaque error. A
  hand-written `mounts.extra` entry keeps working unchanged.

</details>

## `[integration]`

Controls which host resources are shared with the container.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `wayland` | bool | `true` | Share Wayland socket for GUI apps |
| `audio` | bool | `true` | Share PipeWire/PulseAudio sockets |
| `gpu` | string/bool | `"auto"` | GPU passthrough (`true`, `false`, `"auto"`, `"nvidia"`) |
| `dbus` | bool | `true` | Enable D-Bus session bus access |
| `notify` | bool | `false` | Desktop notification forwarding |
| `xdg_open` | bool | `false` | URI opening via host (`xdg-open`) |
| `clipboard` | bool | `false` | Clipboard sharing |
| `sync_fonts` | bool | `false` | Bind-mount `~/.fonts` and `~/.local/share/fonts` (read-only) when present on the host |
| `sync_icons` | bool | `false` | Bind-mount `~/.icons` and `~/.local/share/icons` (read-only) when present on the host |
| `sync_themes` | bool | `false` | Bind-mount `~/.themes` and `~/.local/share/themes` (read-only) when present on the host |
| `gpg_agent` | bool | `false` | Forward GPG agent socket (`S.gpg-agent`). Sets `GPG_TTY` and `GNUPGHOME` |
| `git_identity` | bool | `true` | Bridge host Git identity and `safe.directory` into the container. Needs `git` in the image — podbox never installs it, so this is a no-op without it |
| `host_exec` | table | `{ enabled = false }` | Host command execution (see [`[integration.host_exec]`](#integrationhost_exec) below) |
| `ssh_agent` | bool | `false` | Forward SSH agent socket (`$SSH_AUTH_SOCK`). Requires Podman ≥ 5.6 |

### `GpuMode` values

| TOML value | Meaning |
|------------|---------|
| `"auto"` (default) | Detect available GPU devices at runtime |
| `true` | Enable `/dev/dri` (Intel/AMD) |
| `false` | Disable all GPU passthrough |
| `"nvidia"` | Enable `/dev/dri` + NVIDIA device nodes |

### `[integration.hardware]`

Device passthrough. The container has no devices until you pass them here.
Each becomes an optional `AddDevice=-…` — a host without it skips the device
instead of failing to start.

| Key | Type | Default | Passed through |
|-----|------|---------|----------------|
| `kvm` | bool | `false` | `/dev/kvm` — nested virtualisation, Android emulators |
| `joystick` | bool | `false` | `/dev/input`, `/dev/uinput` — gamepads and joysticks |
| `webcam` | bool | `false` | `/dev/video*`, `/dev/media*` |
| `serial` | bool | `false` | `/dev/ttyUSB*`, `/dev/ttyACM*` — microcontrollers |
| `yubikey` | bool | `false` | `pcscd` socket and `/dev/hidraw*` — smartcards, 2FA |

```toml
[integration.hardware]
webcam = true
kvm = true
```

`podbox doctor` reports when the host is missing a device these would need.

### `[integration.host_exec]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Allow container to execute commands on the host |
| `allowlist` | table | (none) | Allowed commands as alias → path pairs. When set, only those commands may run; when absent, anything may run |

**Example — restrict to `git` and `systemctl`:**
```toml
[integration.host_exec]
enabled = true
allowlist = { git = "/usr/bin/git", systemctl = "/usr/bin/systemctl" }
```

**Security note:** no shell is involved (`execve` directly), so shell
injection can't happen. But the filter is a blocklist — **an allowlisted
binary keeps its full host powers** (`git -C /root …`, `find -exec …`,
`python -c …`). Prefer wrapper scripts that pin the exact arguments over
general-purpose tools. The filter also rejects harmless arguments containing
globs or parentheses.

### `[integration.xdg_dirs]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `documents` | bool | `false` | Mount host `~/Documents` |
| `downloads` | bool | `false` | Mount host `~/Downloads` |
| `pictures` | bool | `false` | Mount host `~/Pictures` |
| `music` | bool | `false` | Mount host `~/Music` |
| `videos` | bool | `false` | Mount host `~/Videos` |
| `desktop` | bool | `false` | Mount host `~/Desktop` |
| `projects` | bool | `false` | Mount host `~/Projects` |

### `[integration.export]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `apps` | string[] | `[]` | App `.desktop` files to export (without `.desktop` suffix) |
| `bins` | string[] | `[]` | Binary shims to generate in `~/.local/bin` |

```toml
[integration]
wayland    = true
audio      = true
gpu        = "auto"
dbus       = true
notify     = true
xdg_open   = true
clipboard  = true
ssh_agent  = true
sync_fonts = true
sync_icons = true
sync_themes = true

[integration.host_exec]
enabled = true
allowlist = { git = "/usr/bin/git" }

[integration.xdg_dirs]
documents = true
downloads = true
projects = true

[integration.export]
apps = ["gedit", "nautilus"]
bins = ["rg", "gcc"]
```

## `[lifecycle]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `quadlet` | bool | `false` | Generate Quadlet systemd files on `podbox enable` |
| `autostart` | bool | `false` | Start container on user login (`WantedBy=default.target`) |
| `on_stop` | string | `"keep"` | Container behavior on stop (`"keep"` or `"remove"`) |
| `auto_update` | bool | `false` | Add `Label=io.containers.autoupdate=registry` for auto-updates |
| `auto_checkpoint` | bool | `false` | Tag the current image as `checkpoint-prev` before update or `build --rebuild`; `podbox rollback` restores that image |
| `idle_timeout` | string | `"off"` | Idle timeout before guest daemon exits (`"off"`, `"30s"`, `"5m"`, `"1h") |

```toml
[lifecycle]
quadlet      = true
autostart    = true
on_stop      = "keep"
auto_update  = true
idle_timeout = "off"
```

## `[systemd]`

Custom systemd unit dependencies for the generated Quadlet.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `requires` | string[] | `[]` | Units that must be active before the container (`Requires=`) |
| `after` | string[] | `[]` | Units the container should start after (`After=`) |

```toml
[systemd]
requires = ["postgres.service", "redis.service"]
after    = ["network-online.target"]
```

## `[dbus]`

D-Bus access control via `xdg-dbus-proxy`. Requires `integration.dbus = true`.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `preset` | string | `""` | Preset filling `talk` for you: `"flatpak"`, `"gnome"`, `"kde"`, `"portal"`. Portal names are never granted via `talk` — they come through interface-scoped rules for `notify` / `xdg_open` (see [dbus-proxy.md](dbus-proxy.md)) |
| `talk` | string[] | `[]` | D-Bus services the container can call (two-way). Adding a portal-family name re-grants the full portal surface — a warning is printed |
| `own` | string[] | `[]` | D-Bus services the container can register on the host bus |

```toml
[dbus]
preset = "gnome"
```

```toml
[dbus]
preset = "portal"
talk = [
    "org.freedesktop.Notifications",
    "org.mpris.MediaPlayer2.*",
]
own = [
    "org.mpris.MediaPlayer2.podbox_app",
]
```

See [dbus-proxy.md](dbus-proxy.md) for the full behavior matrix.

## `[wayland]`

The Wayland firewall. A companion service filters which protocol objects the
container may use — screen capture, virtual input, and input methods are
blocked unless you allow them.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `firewall` | bool | `true` | Filter Wayland protocol access through the compositor proxy |
| `blocked_interfaces` | string[] | (see below) | Wayland globals to deny. Replaces the default list when set |

```toml
[wayland]
firewall = true
blocked_interfaces = [
    "zwlr_screencopy_manager_v1",
    "ext_image_copy_capture_v1",
]
```

Setting `blocked_interfaces` replaces the default list entirely — no merging.

<details>
<summary>Default block list</summary>

Screen capture (`zwlr_screencopy_manager_v1`, `ext_image_copy_capture_v1`),
window listing (`ext_foreign_toplevel_list_v1`), virtual pointers
(`zwlr_virtual_pointer_manager_v1`, `zwlr_virtual_pointer_unstable_v1`),
virtual keyboards and input methods (`zwp_virtual_keyboard_manager_v1`,
`zwp_input_method_v1`, `zwp_input_method_v2`, `ext_input_method_v1`), and
fake input (`org_kde_kwin_fake_input`).

</details>

## Full Example

This is a **reference example** showing every available key with sane defaults.
It is **not** a working config — most install lists, env vars, and mounts
are placeholders. Pick only what you need; omitted keys use their defaults.

```toml
# ── Image ──────────────────────────────────────────────
[image]
base = "fedora:44"              # Base image for custom builds
name = "myenv"                  # Image tag name
image = "ghcr.io/user/myenv:latest"  # Prebuilt ref (omit for custom builds)
pull_retry = 3                  # Pull retry count
pull_retry_delay = "5s"         # Delay between pull retries

[image.packages]
install = ["git", "gcc", "ripgrep"]
remove = ["vim-minimal"]
manager = "dnf"                 # auto-detected; override: dnf, apt, pacman, apk, zypper

[image.run]
commands = ["dnf clean all"]    # Extra RUN steps

# ── Container ──────────────────────────────────────────
[container]
name = "myenv"                  # Required; used for unit names and socket paths
home = "~/containers/myenv"     # Required; isolated home directory (~ expands)
shell = "fish"                  # Default login shell
memory = "4G"                   # Memory limit (e.g. "4G", "512M", omitted = unlimited)
cpus = "2.0"                    # CPU limit (e.g. "2.0", "0.5", omitted = unlimited)
reload_cmd = "systemctl reload …"  # systemd ReloadCmd (omitted = none)

[container.mounts]
extra = ["~/Work:/home/user/Work:z"]

[container.env]
EDITOR = "nvim"
TERM = "xterm-256color"
# forward = ["HTTP_PROXY", "AWS_*"]  # Host vars copied in at exec time

# ── Caches ─────────────────────────────────────────────
# Both opt-in, same keys, different sources:
#   shared_caches → podbox volumes, shared between podbox containers
#   host_caches   → a directory you already keep on the host, bind-mounted in
# rustup and the ~/.cargo/bin half of cargo hold libc-bound binaries and are
# never shared.
[storage.shared_caches]
cargo  = true
npm    = true
mbx    = false

[storage.host_caches]
mbx = false                       # ~/.cache/mbx on both sides

[[storage.host_caches.custom]]
name = "zig"
host_path = "~/.cache/zig"
container_path = "~/.cache/zig"

# ── Security ───────────────────────────────────────────
[security]
apparmor = "unconfined"         # AppArmor profile (omitted = none)
seccomp = "default"             # Seccomp profile (omitted = none, "unconfined" = off)
security_label_disable = true   # Disable SELinux labels (needed for Wayland)
no_new_privileges = true        # Block setuid escalation (sudo, su, AUR helpers)
read_only_rootfs = false        # Make rootfs read-only (requires writable volumes)
userns = "keep-id"              # UserNS mode: keep-id, nomap, private (omitted = keep-id)
cap_add = ["SYS_PTRACE"]        # Extra Linux capabilities (omitted = none)
secrets = ["openai_key"]        # Podman secrets; no value ever lands in the image

# [[security.secrets]]         # detailed form when you need type/target/mode
# name = "aws_creds"
# type = "mount"
# mode = "0400"

# ── Network ────────────────────────────────────────────
[network]
mode = "pasta"                  # host, bridge, none, pasta, slirp4netns, private (default: pasta)
ports = ["8080:80"]             # Port mappings (ignored in host mode)

# ── Integration ────────────────────────────────────────
[integration]
wayland     = true              # Share Wayland socket
audio       = true              # Share PipeWire / PulseAudio
gpu         = "auto"            # GPU: true, false, "auto", "nvidia"
dbus        = true              # Enable D-Bus session bus
notify      = true              # Forward desktop notifications
xdg_open    = true              # Forward URI opening (xdg-open)
clipboard   = true              # Clipboard sharing
ssh_agent   = false             # Forward SSH agent (needs Podman ≥ 5.6)
gpg_agent   = false             # Forward GPG agent
sync_fonts  = true              # Sync ~/.fonts / ~/.local/share/fonts (ro)
sync_icons  = true              # Sync ~/.icons / ~/.local/share/icons (ro)
sync_themes = true              # Sync ~/.themes / ~/.local/share/themes (ro)

[integration.hardware]         # Host device passthrough (all opt-in)
webcam = false
kvm = false

[integration.host_exec]
enabled = false
allowlist = { git = "/usr/bin/git" }  # Alias → absolute path (required when enabled)

[integration.xdg_dirs]
documents = false
downloads = false
pictures  = false
music     = false
videos    = false
desktop   = false
projects  = false

[integration.export]
apps = ["gedit", "nautilus"]    # Export .desktop files for these apps
bins = ["rg", "gcc"]            # Create bin shims for these commands

# ── Lifecycle ──────────────────────────────────────────
[lifecycle]
quadlet      = false            # Generate systemd Quadlet files on enable
autostart    = false            # Start container on user login
on_stop      = "keep"           # Container behavior on stop: "keep" or "remove"
auto_update  = false            # Label for auto-updates (registry/local)
idle_timeout = "off"            # Guest daemon idle timeout: "off", "30s", "5m", "1h"

# ── systemd dependencies ────────────────────────────────
[systemd]
requires = ["postgres.service", "redis.service"]
after    = ["network-online.target"]

# ── D-Bus ──────────────────────────────────────────────
[dbus]
preset = "portal"               # Named preset: flatpak, gnome, kde, portal ("" = none)
talk = ["org.freedesktop.Notifications"]
own  = ["org.mpris.MediaPlayer2.podbox_app"]

# ── Wayland firewall ───────────────────────────────────
[wayland]
firewall = true                 # Enable Wayland protocol firewall
blocked_interfaces = [          # Blocked Wayland globals (default list)
    "zwlr_screencopy_manager_v1",
    "ext_image_copy_capture_v1",
]
```

Omitted keys use their defaults. See the tables above for every supported key.
