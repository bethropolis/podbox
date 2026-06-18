---
description: Complete TOML configuration reference for podbox — all keys, defaults, and examples for image, container, integration, lifecycle, and D-Bus settings.
---

# Configuration Reference

`podbox` searches for a definition file in this order:

1. `./.podbox.toml` (project-local)
2. Active context from `~/.config/podbox/.active` (set via `podbox use <name>`)
3. `~/.config/podbox/*.toml` (first file, sorted by name)
4. Embedded default (`fedora:44`, name `podbox`)

---

## `[image]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `base` | string | *required* | Base container image (e.g. `"fedora:41"`) |
| `name` | string | *required* | Image tag name (e.g. `"myenv"`) |
| `pull_retry` | int | `3` | Number of pull retries on failure |
| `pull_retry_delay` | string | `"5s"` | Delay between pull retries |

### `[image.packages]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `install` | string[] | `[]` | Packages to install via `dnf install` |
| `remove` | string[] | `[]` | Packages to remove via `dnf remove` |

### `[image.run]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `commands` | string[] | `[]` | Extra `RUN` commands in the Containerfile |

```toml
[image]
base = "fedora:41"
name = "myenv"

[image.packages]
install = ["git", "gcc", "ripgrep"]
remove = ["vim-minimal"]

[image.run]
commands = ["dnf clean all"]
```

---

## `[container]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `name` | string | *required* | Container name (used for systemd unit names, socket paths) |
| `home` | string | *required* | Host path for isolated home (`~` expands) |
| `shell` | string | `"fish"` | Default login shell inside the container |
| `memory` | string | — | Memory limit (e.g. `"4G"`, `"2048M"`). Passed as `Memory=` in Quadlet |
| `cpus` | string | — | CPU limit (e.g. `"2.0"`, `"0.5"`). Converted to `CpuQuota=` in Quadlet |
| `reload_cmd` | string | — | Command run on config reload. Passed as `ReloadCmd=` in Quadlet |

### `[container.mounts]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `extra` | string[] | `[]` | Extra `Volume=` lines (e.g. `"~/Work:/home/user/Work:z"`) |

### `[container.env]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `*` | string | — | Arbitrary environment variables passed to the container |

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
```

---

## `[security]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `apparmor` | string | — | AppArmor profile name. Passed as `AppArmor=` in Quadlet (`"unconfined"` to disable) |
| `seccomp` | string | — | Seccomp profile path, `"default"`, or `"unconfined"`. Passed as `SeccompProfile=` |
| `security_label_disable` | bool | `true` | Disable SELinux process labeling. Emits `SecurityLabelDisable=true` when set |
| `no_new_privileges` | bool | `true` | Block privilege escalation via setuid binaries (`sudo`, `su`, AUR helpers). Emits `NoNewPrivileges=true` in the Quadlet. Set `false` to allow. |
| `read_only_rootfs` | bool | `false` | Make root filesystem read-only. Emits `ReadOnly=true` in Quadlet |
| `userns` | string | — | User namespace mode override. Defaults to `"keep-id"`. Supported: `"keep-id"`, `"nomap"`, `"private"` |
| `cap_profile` | string | `"default"` | Capability preset. Options: `"none"`, `"default"`, `"monitoring"`, `"admin"`. Adds a predefined set of `--cap-add` entries alongside any `cap_add` list below |
| `cap_add` | string[] | `[]` | Extra Linux capabilities to add (e.g. `["SYS_ADMIN"]`). Combined with `cap_profile` caps |

```toml
[security]
apparmor = "unconfined"
seccomp = "default"
read_only_rootfs = true
userns = "nomap"
cap_profile = "monitoring"
cap_add = ["SYS_ADMIN"]
```

---

## `[network]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `mode` | string | `"host"` | Network mode. Supported: `"host"`, `"bridge"`, `"none"`, `"pasta"`, `"slirp4netns"`, `"private"`. Passed as `Network=` in Quadlet |
| `ports` | string[] | `[]` | Port mappings (`"hostPort:containerPort"`). Emitted as `PublishPort=` in Quadlet (ignored in `host` mode) |

```toml
[network]
mode = "pasta"
ports = ["8080:80", "443:443"]
```

---

## `[integration]`

Controls which host resources are shared with the container.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `wayland` | bool | `true` | Share Wayland socket for GUI apps |
| `audio` | bool | `true` | Share PipeWire/PulseAudio sockets |
| `gpu` | string/bool | `"auto"` | GPU passthrough (`true`, `false`, `"auto"`, `"nvidia"`) |
| `dbus` | bool | `true` | Enable D-Bus session bus access |
| `notify` | bool | `true` | Desktop notification forwarding |
| `xdg_open` | bool | `true` | URI opening via host (`xdg-open`) |
| `clipboard` | bool | `true` | Clipboard sharing |
| `sync_fonts` | bool | `true` | Bind-mount `~/.fonts` and `~/.local/share/fonts` (read-only) when present on the host |
| `sync_icons` | bool | `true` | Bind-mount `~/.icons` and `~/.local/share/icons` (read-only) when present on the host |
| `sync_themes` | bool | `true` | Bind-mount `~/.themes` and `~/.local/share/themes` (read-only) when present on the host |
| `gpg_agent` | bool | `false` | Forward GPG agent socket (`S.gpg-agent`). Sets `GPG_TTY` and `GNUPGHOME` |
| `host_exec` | table | `{ enabled = false }` | Host command execution (see [`[integration.host_exec]`](#integrationhost_exec) below) |
| `ssh_agent` | bool | `false` | Forward SSH agent socket (`$SSH_AUTH_SOCK`). Requires Podman ≥ 5.6 |

### `GpuMode` values

| TOML value | Meaning |
|------------|---------|
| `"auto"` (default) | Detect available GPU devices at runtime |
| `true` | Enable `/dev/dri` (Intel/AMD) |
| `false` | Disable all GPU passthrough |
| `"nvidia"` | Enable `/dev/dri` + NVIDIA device nodes |

### `[integration.host_exec]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Allow container to execute commands on the host |
| `allowlist` | table | (none) | Alias → absolute-path map for allowed commands. When set, only those commands may be run (resolved via the mapped host path, ignoring the guest's `$PATH`). When absent, any command is allowed (legacy mode). |

**Example — restrict to `git` and `systemctl`:**
```toml
[integration.host_exec]
enabled = true
allowlist = { git = "/usr/bin/git", systemctl = "/usr/bin/systemctl" }
```

**Security note:** Even with an allowlist, argument injection can subvert a binary (e.g. `git --exec-path=…`). The host automatically rejects arguments containing shell metacharacters (`;`, `|`, `&`, `$`, `` ` ``) or dangerous flag patterns (`--exec-path`, `--config`, `-o`, etc.).

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

---

## `[lifecycle]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `quadlet` | bool | `false` | Generate Quadlet systemd files on `podbox enable` |
| `autostart` | bool | `false` | Start container on user login (`WantedBy=default.target`) |
| `on_stop` | string | `"keep"` | Container behavior on stop (`"keep"` or `"remove"`) |
| `auto_update` | bool | `false` | Add `Label=io.containers.autoupdate=registry` for auto-updates |
| `idle_timeout` | string | `"off"` | Idle timeout before guest daemon exits (`"off"`, `"30s"`, `"5m"`, `"1h") |

```toml
[lifecycle]
quadlet      = true
autostart    = true
on_stop      = "keep"
auto_update  = true
idle_timeout = "off"
```

---

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

---

## `[dbus]`

D-Bus access control via `xdg-dbus-proxy`. Requires `integration.dbus = true`.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `preset` | string | `""` | Named preset to expand into talk rules. Supported: `"flatpak"`, `"gnome"`, `"kde"`, `"portal"`. When set, auto-fills `talk` with the preset's service names |
| `talk` | string[] | `[]` | D-Bus services the container can call (two-way) |
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

See [dbus-proxy.md](dbus-proxy.md) for details.

### Behavior matrix

| `integration.dbus` | `[dbus]` talk/own | Result |
|--------------------|-------------------|--------|
| `false` | any | No D-Bus access |
| `true` | empty (default) | Unfiltered `Volume=%t/bus` |
| `true` | populated | Proxy socket via `xdg-dbus-proxy` |

---

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

# ── Security ───────────────────────────────────────────
[security]
apparmor = "unconfined"         # AppArmor profile (omitted = none)
seccomp = "default"             # Seccomp profile (omitted = none, "unconfined" = off)
security_label_disable = true   # Disable SELinux labels (needed for Wayland)
no_new_privileges = true        # Block setuid escalation (sudo, su, AUR helpers)
read_only_rootfs = false        # Make rootfs read-only (requires writable volumes)
userns = "keep-id"              # UserNS mode: keep-id, nomap, private (omitted = keep-id)
cap_add = ["SYS_PTRACE"]        # Extra Linux capabilities (omitted = none)

# ── Network ────────────────────────────────────────────
[network]
mode = "host"                   # host, bridge, none, pasta, slirp4netns, private
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
