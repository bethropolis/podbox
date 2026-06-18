---
description: How podbox works — codegen pipeline, generated Containerfile and Quadlet units, host-guest socket protocol, UID mapping, and project structure.
---

# Architecture

## How It Works

A definition TOML is the single source of truth. Everything podbox generates —
Containerfiles, Quadlet systemd units, lock files, desktop entries — derives
from this one file. The user never writes a raw Containerfile or systemd unit
manually.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/how_it_works.svg">
    <img src="assets/how_it_works.svg" alt="How podbox Works" width="100%" style="max-width: 820px;">
  </picture>
</p>

## Codegen Pipeline

`podbox build` runs these steps in order. Each codegen step is a **pure function**:
data in, string out, no I/O. Orchestration (file writes, podman invocations) is
separate.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/codegen_pipeline.svg">
    <img src="assets/codegen_pipeline.svg" alt="Codegen Pipeline" width="100%" style="max-width: 820px;">
  </picture>
</p>

## Generated Containerfile

```dockerfile
FROM fedora:44

# [image.packages]
RUN dnf install -y git gcc ripgrep && dnf clean all

# [image.run] custom steps
RUN dnf clean all

# podbox integration layer — always last
COPY podbox-guest /usr/local/bin/podbox-guest
RUN chmod +x /usr/local/bin/podbox-guest

ENV PODBOX_CONTAINER=myenv
ENV PODBOX_HOST_VERSION=0.5.0
ENV LANG=en_US.UTF-8
ENV LC_ALL=en_US.UTF-8
ENTRYPOINT ["/usr/local/bin/podbox-guest", "--entry"]
CMD ["/usr/bin/fish"]
```

### Build Context Layout

```
~/.local/share/podbox/<name>/
├── Containerfile
├── podbox-guest          # static musl binary from host
```

## Generated Quadlet Files

Files written to `~/.config/containers/systemd/`:

### `myenv.build`

```ini
[Build]
ImageTag=localhost/podbox-myenv:latest
File=/home/user/.local/share/podbox/myenv/Containerfile
```

The `.build` unit makes `myenv.service` depend on the build. Images are only
rebuilt when the Containerfile changes.

### `myenv.socket`

```ini
[Unit]
Description=podbox host-guest socket — myenv

[Socket]
ListenStream=%t/podbox/myenv.sock
Service=myenv-host.service
SocketMode=0600
DirectoryMode=0700

[Install]
WantedBy=sockets.target
```

`%t` is systemd's specifier for `$XDG_RUNTIME_DIR`. The socket is created
before the container starts and persists across restarts.

### `myenv.container`

Key Quadlet settings (see [quadlet.md](quadlet.md) for full list):

| Setting | Value | Purpose |
|---------|-------|---------|
| `UserNS` | `keep-id` | Maps host UID/GID into container |
| `User` | `root` | Run as root (UID mapped via UserNS) |
| `SecurityLabelDisable` | `true` | Required for Wayland socket access |
| `NoNewPrivileges` | `true` | Block setuid escalation (sudo, su) |
| `PodmanArgs` | `--init` | catatonit as PID 1 (zombie reaping) |
| `PodmanArgs` | `--workdir=/home/%u` | Default working directory |
| `Volume` | `<context>/.flatpak-info:/.flatpak-info:ro` | Sandbox detection marker (portals) |
| `Volume` | `%h/containers/<name>:/home/%u:Z` | Isolated home (never the host home) |
| `Volume` | `%t/podbox/<name>.sock:%t/podbox/<name>.sock` | Host-guest socket |
| `Environment` | `HOST_USER`, `HOST_UID`, `HOST_GID` | Host identity injected |
| `Environment` | `PATH=/run/podbox/bin:…` | Interceptor directory prepended |
| `Restart` | `on-failure` | Auto-restart on crash |

Volumes for Wayland, audio, D-Bus, XDG dirs, GPU devices, and theme/icon/font
sync are added conditionally based on the config.

## Host-Guest Socket Protocol

The guest daemon connects to a Unix socket on the host to bridge container
capabilities. Messages are length-prefixed JSON (see [protocol.md](protocol.md)
for the wire format).

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/socket_protocol.svg">
    <img src="assets/socket_protocol.svg" alt="Host-Guest Socket Protocol" width="100%" style="max-width: 820px;">
  </picture>
</p>

## Guest Daemon (podbox-guest)

The guest binary is a static musl binary baked into every built image.
Its behavior is determined by `argv[0]`:

| Invoked as | Mode |
|-----------|------|
| `podbox-guest --entry` | Fork daemon, exec user shell/command |
| `podbox-guest --daemon` | Event loop, interceptor setup |
| `notify-send` (symlink) | Parse args, forward to daemon |
| `xdg-open` (symlink) | Parse args, forward to daemon |
| `podbox-clipboard` (symlink) | Read stdin / write stdout for clipboard |
| `host-exec` (symlink) | Execute command on host, relay output |

### Daemon startup sequence

1. Read `PODBOX_CONTAINER` env → derive socket paths
2. Create `/run/podbox/bin/` directory
3. Check version drift — compare `PODBOX_HOST_VERSION` against podbox-guest version
4. Connect to host socket (3 retries × 500ms)
5. Handshake: send capabilities, receive accepted list and idle timeout
6. Install interceptor symlinks in `/run/podbox/bin/` for accepted capabilities
7. Prepend `/run/podbox/bin` to `$PATH` via `/etc/profile.d/podbox.sh` and `/etc/fish/conf.d/podbox.fish`
8. Enter event loop (poll + pidfd-based, 0% CPU when idle, configurable idle timeout)

If the socket is absent at startup, the daemon logs a warning and exits cleanly.
The container continues running without integration — this is intentional.

## UID Mapping

`UserNS=keep-id` + `User=root` creates an idmapped mount that shifts UIDs by 1
inside the container (host UID 1000 → container UID 999). The entrypoint reads
the actual home owner and makes the directory world-writable. No `chown` is
performed on bind-mounted directories — that would corrupt host ownership
through the idmapped mount.

## Runtime Flow (Full Sequence)

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/runtime_flow.svg">
    <img src="assets/runtime_flow.svg" alt="Runtime Flow Sequence" width="100%" style="max-width: 820px;">
  </picture>
</p>

## Project Structure

```
podbox/
├── Cargo.toml                    # workspace root
├── crates/
│   ├── podbox/                   # host CLI binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs           # entry point, dispatch
│   │       ├── lib.rs            # module declarations
│   │       ├── cli.rs            # clap CLI definition
│   │       ├── build.rs          # Containerfile generation + build orchestration
│   │       ├── cli.rs            # clap CLI definition
│   │       ├── compositor.rs     # Wayland firewall proxy
│   │       ├── config/           # TOML parsing, types, validation, defaults
│   │       │   ├── mod.rs
│   │       │   ├── types.rs      # all config structs
│   │       │   ├── enums.rs      # PackageManager, GpuMode, OnStop, XdgDirValue
│   │       │   ├── fs.rs         # config discovery, active context
│   │       │   ├── defaults.rs   # embedded default + helper functions
│   │       │   └── validation.rs # config validation
│   │       ├── codegen/          # pure string generators
│   │       │   ├── mod.rs
│   │       │   ├── quadlet.rs    # .container, .build, .socket, .service gen
│   │       │   ├── containerfile.rs# Containerfile generation
│   │       │   └── distros.rs    # distro family detection, base packages
│   │       ├── commands/         # command implementations
│   │       │   ├── mod.rs
│   │       │   ├── clone.rs
│   │       │   ├── context.rs
│   │       │   ├── create.rs
│   │       │   ├── definition.rs
│   │       │   ├── diff.rs
│   │       │   ├── export.rs
│   │       │   ├── inspect.rs
│   │       │   ├── lifecycle.rs  # build, enable, disable, start, stop, remove
│   │       │   ├── pull.rs
│   │       │   ├── runtime.rs    # shell, enter, exec, run, status, logs
│   │       │   ├── serve.rs
│   │       │   ├── stats.rs
│   │       │   └── translate.rs
│   │       ├── diff.rs           # package drift detection
│   │       ├── editor.rs         # editor resolution
│   │       ├── env.rs            # host env resolution (GPU, audio, locale)
│   │       ├── error.rs          # error types
│   │       ├── export.rs         # .desktop + bin shim export
│   │       ├── guest.rs          # guest binary installation
│   │       ├── labels.rs         # image label defaults
│   │       ├── lock.rs           # build lock file
│   │       ├── podman.rs         # version detection + subcommand wrappers
│   │       ├── process.rs        # exec_replace, run_piped, spawn
│   │       ├── profiles.rs       # named config templates (bundled + custom)
│   │       ├── profiles/         # built-in profile TOMLs
│   │       │   ├── cachy.toml
│   │       │   ├── dev.toml
│   │       │   └── fedora.toml
│   │       ├── protocol.rs       # host-side protocol handler
│   │       ├── quadlet_install.rs# Quadlet file installation
│   │       ├── socket_host/      # host-side socket handler
│   │       │   ├── handlers.rs
│   │       ├── systemd.rs        # systemctl wrappers
│   │       ├── wizard/           # interactive setup wizard
│   │       │   ├── mod.rs
│   │       │   ├── prompts.rs
│   │       │   ├── shell.rs
│   │       │   └── summary.rs
│   │       └── xdg.rs            # XDG dir resolution
│   │
│   ├── podbox-guest/             # static musl sidecar
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs           # argv[0] dispatch
│   │       ├── lib.rs            # module declarations
│   │       ├── entry.rs          # fork + exec
│   │       ├── daemon.rs         # event loop (poll + pidfd)
│   │       ├── socket.rs         # socket I/O
│   │       ├── protocol.rs       # message types + framing (re-exports)
│   │       ├── interceptors/     # notify, xdg_open, clipboard, host_exec
│   │       └── error.rs
│   │
│   └── podbox-protocol/          # shared wire-format types
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs            # GuestMessage, HostMessage, read/write_frame
│
├── tests/                        # integration + unit tests
├── scripts/                      # install / uninstall
└── docs/                         # documentation
```

### Key architectural rules

- **Pure codegen:** All `codegen::*` functions are pure — data in, string out.
  No I/O, no env reads, no filesystem access.
- **Boundary separation:** I/O lives only in `commands/`, `build.rs`,
  `quadlet_install.rs`, `socket_host.rs`, `export.rs`.
- **musl static:** `podbox-guest` must stay statically linkable. No tokio,
  no openssl, no crate that links against glibc. Uses `poll()` + pidfds.
- **exec_replace for TTY:** `podbox shell` and `podbox exec` use
  `CommandExt::exec()` to replace the process — never `spawn_interactive`.
  This preserves the TTY for readline, Ctrl+L, etc.
- **pidfd-based process tracking:** The guest daemon uses `pidfd_open()` (Linux 5.3+)
  and `poll()` to watch user process exits without busy-looping.
- **Config as single source of truth:** Containerfile, Quadlet units, lock files,
  and desktop entries all derive from one TOML definition.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Configuration error |
| 3 | Container missing |
| 4 | Build or inspect failure |
| 5 | Missing dependency (podman not found) |
| 6 | Pull or tag failure |
