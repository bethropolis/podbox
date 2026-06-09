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
| `shell` | string | `"bash"` | Default login shell inside the container |
| `memory` | string | — | Memory limit (e.g. `"4G"`, `"2048M"`). Passed as `Memory=` in Quadlet |
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
| `no_new_privileges` | bool | `false` | Allow privilege escalation (`sudo`, `su`, AUR helpers). Set `true` to block with `NoNewPrivileges` |

```toml
[security]
apparmor = "unconfined"
seccomp = "default"
no_new_privileges = false
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
| `notify` | bool | `false` | Desktop notification forwarding |
| `xdg_open` | bool | `false` | URI opening via host (`xdg-open`) |
| `clipboard` | bool | `false` | Clipboard sharing |
| `sync_fonts` | bool | `false` | Bind-mount `~/.fonts` and `~/.local/share/fonts` (read-only) when present on the host |
| `sync_icons` | bool | `false` | Bind-mount `~/.icons` and `~/.local/share/icons` (read-only) when present on the host |
| `sync_themes` | bool | `false` | Bind-mount `~/.themes` and `~/.local/share/themes` (read-only) when present on the host |
| `host_exec` | table | `{ enabled = false }` | Host command execution (see [`[integration.host_exec]`](#integrationhost_exec) below) |
| `ssh_agent` | bool | `false` | Forward SSH agent socket (`$SSH_AUTH_SOCK`). Requires Podman ≥ 5.6 |
| `gpg_agent` | bool | `false` | Forward GPG agent socket (`S.gpg-agent`). Sets `GPG_TTY` and `GNUPGHOME` |

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

```toml
[lifecycle]
quadlet     = true
autostart   = true
on_stop     = "keep"
auto_update = true
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

A typical config is ~25 lines. Empty/default sections are omitted automatically.

```toml
[image]
base = "fedora:44"
name = "myenv"

[image.packages]
install = ["git", "gcc", "ripgrep"]

[container]
name = "myenv"
home = "~/containers/myenv"
shell = "bash"

[integration]
wayland = true
audio = true
gpu = "auto"

[integration.xdg_dirs]
documents = true
downloads = true
```

For a reference of every supported key, see the tables above.
