# Quadlet Keys Used

## `.build` file

| Key | Value | Notes |
|-----|-------|-------|
| `ImageTag` | `localhost/podmgr-<name>:latest` | Local tag for built image |
| `File` | Absolute path to Containerfile | Must be absolute |

## `.socket` file

| Key | Value | Notes |
|-----|-------|-------|
| `ListenStream` | `%t/podmgr/<name>.sock` | `%t` = `$XDG_RUNTIME_DIR` |
| `SocketMode` | `0600` | User-only access |
| `DirectoryMode` | `0700` | Parent dir permissions |

## `.container` file

| Key | Value | Notes |
|-----|-------|-------|
| `Image` | `podmgr-<name>.build` | References the `.build` unit |
| `ContainerName` | `<name>` | Podman container name |
| `UserNS` | `keep-id` | Maps host UID/GID |
| `SecurityLabelDisable` | `true` | Required for Wayland socket |
| `Volume` | `%h/containers/<name>:/root:Z` | Isolated home (NOT host home) |
| `Volume` | `%t/wayland-0:%t/wayland-0` | Wayland socket (conditional) |
| `Volume` | `%t/pulse:%t/pulse` | PulseAudio (conditional) |
| `Volume` | `%t/bus:%t/bus` | D-Bus session (conditional) |
| `Volume` | `%t/podmgr/<name>.sock:%t/...` | Host-guest socket |
| `Environment` | `WAYLAND_DISPLAY=...` | Wayland display name |
| `Environment` | `PULSE_SERVER=unix:%t/pulse/native` | Pulse server path |
| `Environment` | `DBUS_SESSION_BUS_ADDRESS=...` | D-Bus address |
| `AddDevice` | `/dev/dri` | GPU (conditional on `gpu=true`) |
| `PodmanArgs` | `--init` | catatonit as PID 1 |
| `Restart` | `on-failure` | Auto-restart on crash |
| `WantedBy` | `default.target` | Autostart (conditional) |

## Important Notes

- `%t` is the systemd specifier for `$XDG_RUNTIME_DIR` — never substitute it.
- `%h` is the systemd specifier for the user's home — never substitute it.
- Files go in `~/.config/containers/systemd/`, NOT `~/.config/systemd/user/`.
