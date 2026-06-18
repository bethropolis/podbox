---
description: Get started with podbox — prebuilt profiles, custom builds, and daily usage.
---

# Getting Started

## Installation

```bash
curl -fsSL https://bethropolis.github.io/podbox/install.sh | sh
```

Or from source: `git clone https://github.com/bethropolis/podbox && cd podbox && scripts/install.sh`.

See the [README](../README.md#requirements) for system requirements.

---

## Two Ways to Create a Container

podbox supports two workflows depending on how much control you need:

| Method | Use case | How it works |
|--------|----------|-------------|
| **Prebuilt** | Quick start, gaming, reproducible | Pull a ready-made image from a registry. Packages and config are baked in — you just create and enter. |
| **Custom** | Full control, specific packages | Build from a distro base image via Containerfile. You declare everything in the TOML config and podbox generates the rest. |

---

## Prebuilt (Quick Start)

Prebuilt profiles come with Wayland, audio, GPU, and common packages ready to go.
They're the fastest way to get a working container.

### Available profiles

| Profile | Base | Use case |
|---------|------|----------|
| `cachy` | CachyOS (Arch-based) | Gaming, general purpose |
| `fedora` | Fedora | Development, general purpose |
| `dev` | Fedora | Development tooling, focused toolset |

Run `podbox init` to see the full list.

### Non-interactive

```bash
# Create a gaming-ready container
podbox create cachy

# Or a Fedora-based one with a custom name
podbox create fedora --name dev
```

### Interactive

```bash
# Launch the wizard and pick a profile
podbox init -i

# After the wizard finishes, create the container
podbox create dev
```

### Verify it works

```bash
# List all podbox containers
podbox list

# Check status
podbox status cachy

# Run diagnostics
podbox doctor
```

### What happens

1. `podbox init` creates a config file at `~/.config/podbox/<name>.toml`
2. `podbox create` pulls the prebuilt image, writes Quadlet systemd files, and starts the container
3. The guest daemon (`podbox-guest`) starts inside and connects to the host for notifications, clipboard, and URI forwarding
4. The container is running and ready — `podbox enter <name>` drops you into a shell

---

## Custom (Build from Base)

Build a container from a plain distro image with your own packages, shell, and configuration.

### Non-interactive

```bash
# Create a config from a base image
podbox init fedora:44 --name myenv

# Build the image, enable Quadlet, and start
podbox create myenv

# Jump in
podbox enter myenv
```

Or in one step with `create`:

```bash
# podbox create works with any OCI image reference
podbox create myenv
podbox create ubuntu:24.04 --name dev
```

### Interactive

```bash
# Launch the interactive wizard
podbox init -i

# Select "Custom (from scratch)" at the top of the list
# Choose: base image, packages to install, extra RUN commands
# Complete the wizard (shell, XDG dirs, GPU, lifecycle)

# Build and start
podbox create myenv
```

### Container naming

When `podbox init <image>` is called without `--name`, the container name is derived from the image tag:

| Image ref | Container name |
|-----------|---------------|
| `fedora:44` | `fedora-44` |
| `fedora:latest` | `fedora` |
| `ubuntu:24.04` | `ubuntu-24-04` |
| `ghcr.io/user/img:v1` | `img-v1` |

This avoids name conflicts when creating containers from different tags of the same base image. Use `--name` to override explicitly.

### Custom config example

```toml
# ~/.config/podbox/myenv.toml
[image]
base = "fedora:44"
name = "myenv"

[image.packages]
install = ["fish", "git", "neovim", "gcc", "ripgrep"]

[container]
name = "myenv"
home = "~/containers/myenv"
shell = "/usr/bin/fish"

[integration]
wayland = true
audio = true
gpu = "auto"

[integration.xdg_dirs]
documents = true
downloads = true
projects = true
```

!!! tip ""
    Empty or default sections (`[lifecycle]`, `[dbus]`, `[container.env]`, etc.) are omitted automatically — the generated TOML stays concise.

### Inspect what was generated

```bash
# View the resolved TOML config
podbox inspect myenv --config

# View the generated Quadlet systemd units
podbox inspect myenv --quadlet

# View the computed environment
podbox inspect myenv --env
```

### Check for drift

After installing packages manually inside the container, see what differs from your config:

```bash
podbox diff myenv
```

Use `--apply` to update the config TOML's install list to match the running container:

```bash
podbox diff myenv --apply
```

### What happens

1. `podbox init` creates a config file at `~/.config/podbox/<name>.toml`
2. `podbox build` auto-generates a Containerfile from the config, copies in the guest binary (`podbox-guest`), and runs `podman build`
3. `podbox enable` writes Quadlet files (`<name>.container`, `<name>.socket`, `<name>-host.service`) to `~/.config/containers/systemd/`
4. `podbox start` starts the container — the guest daemon connects to the host socket
5. `podbox enter <name>` opens an interactive shell

---

## Daily Usage

### Active context

Set a default container so bare commands "just work":

```bash
# Set myenv as the active context
podbox use myenv

# All commands now target myenv
podbox status
podbox logs
podbox exec -- htop
```

To target a different container, pass the name explicitly:

```bash
podbox status fedora
podbox enter fedora
```

### Open a shell

```bash
podbox enter myenv
podbox shell myenv
```

Both work. `enter` is an alias for `shell`.

### Run commands

```bash
# Run interactively inside the container
podbox exec -- htop
podbox exec -- cargo build

# Run as root
podbox exec --root -- apt update

# Launch a GUI app (detached)
podbox run firefox
podbox run gedit ~/notes.txt
```

### Export to host

Make container apps and binaries available on the host:

```bash
# Add Firefox to your host launcher
podbox export app firefox

# Make ripgrep available as a host command
podbox export bin rg

# Remove all exports for the current container
podbox export clean
```

!!! tip ""
    Exported `.desktop` files go to `~/.local/share/applications/` and binary shims to `~/.local/bin/`.

### Resource usage

```bash
# Show real-time resource usage
podbox stats

# Single snapshot, no streaming
podbox stats --no-stream

# JSON output for scripting
podbox stats --output json
```

### Snapshots

Commit the current container state and roll back if needed:

```bash
# Tag the current state (defaults to timestamp tag)
podbox snapshot myenv

# Tag with a custom name
podbox snapshot myenv --tag before-upgrade

# Restore to a previous state
podbox restore myenv before-upgrade
```

### Path translation

Find the equivalent path between host and container:

```bash
# Host → container
podbox translate-path --to-container ~/Projects/myapp

# Container → host
podbox translate-path --to-host /home/user/Projects/myapp
```

### Troubleshoot

```bash
# Run diagnostics
podbox doctor

# Auto-fix common issues (Wayland socket ownership, etc.)
podbox doctor --fix
```

---

## Lifecycle Management

### Understanding the chain

podbox uses four stages:

```bash
podbox build    # Build the container image from the TOML config
podbox enable   # Write Quadlet systemd files (~/.config/containers/systemd/)
podbox start    # Start the container
podbox enter    # Open a shell
```

`podbox create` runs all of these in one command.

### Preview without side effects

```bash
# See what would happen without executing anything
podbox build --dry-run
podbox enable --dry-run
```

### Quadlet persistence

When Quadlet is enabled (`[lifecycle] quadlet = true`), systemd manages the
container:

- It starts automatically on login (`WantedBy=default.target`)
- It restarts on crash (`Restart=on-failure`)
- The socket is created before the container and persists across restarts

### Start and stop

```bash
# Start
podbox start myenv

# Stop (container stays, can be started again)
podbox stop myenv

# Disable and remove Quadlet files
podbox disable myenv

# Force-disable without loading the config
podbox disable myenv --force
```

### Remove

```bash
# Remove the container only (config stays)
podbox remove myenv

# Remove container and home directory
podbox remove myenv --all

# Force-remove without confirmation
podbox remove myenv --all --force

# Also delete the TOML config file
podbox remove myenv --config

# Clean up orphaned/failed containers
podbox remove --stale
```

### Update and rebuild

```bash
# Rebuild the image (picks up config changes automatically)
podbox build

# Force a full rebuild from scratch
podbox build --rebuild

# Pull latest image and restart (prebuilt containers)
podbox update myenv

# Pull without restarting
podbox update myenv --no-restart
```

### Edit config interactively

```bash
# Open the config in your editor
podbox edit myenv

# After saving, rebuild if the image config changed
podbox edit myenv --rebuild
```

---

## Commands at a Glance

### Creating and building

| Command | Description |
|---------|-------------|
| `podbox init` | List available profiles |
| `podbox init <image>` | Scaffold a custom config from a base image |
| `podbox init -i` | Interactive wizard (custom or profile) |
| `podbox init --profile <name>` | Scaffold from a prebuilt profile |
| `podbox create <name>` | Init → build → enable → start in one step |
| `podbox create <image> --name <n>` | Pull + create config + enable + start |
| `podbox build [<name>]` | Build or rebuild the container image |
| `podbox pull <name>` | Pull a prebuilt image without building |

### Running and entering

| Command | Description |
|---------|-------------|
| `podbox enter [<name>]` | Enter a running container (auto-starts) |
| `podbox shell [<name>]` | Open an interactive shell |
| `podbox exec -- <cmd>` | Execute a command |
| `podbox run <app>` | Launch a GUI app (detached) |

### Managing state

| Command | Description |
|---------|-------------|
| `podbox enable [<name>]` | Install Quadlet systemd files |
| `podbox disable [<name>] [--force]` | Remove Quadlet files |
| `podbox start [<name>]` | Start the container |
| `podbox stop [<name>]` | Stop the container |
| `podbox remove [<name>] [--all]` | Remove the container (and home with `--all`) |
| `podbox remove --stale` | Clean up orphaned/failed containers |
| `podbox snapshot [<name>] [--tag <t>]` | Commit container state as an OCI image |
| `podbox restore <tag> [<name>]` | Roll back to a previous snapshot |
| `podbox clone <src> <dst>` | Copy a config for a variant |
| `podbox update [<name>]` | Pull latest image and restart |

### Exporting

| Command | Description |
|---------|-------------|
| `podbox export app <name>` | Export a `.desktop` file to the host launcher |
| `podbox export bin <name>` | Create a binary shim in `~/.local/bin` |
| `podbox export clean` | Remove all exported shims and `.desktop` files |

### Diagnostics and utilities

| Command | Description |
|---------|-------------|
| `podbox status [<name>]` | Show container state |
| `podbox logs [<name>] [-f] [--since <time>]` | Show container logs |
| `podbox stats [<name>]` | Show resource usage |
| `podbox diff [<name>]` | Compare installed packages against config |
| `podbox doctor [--fix]` | Diagnose and fix common issues |
| `podbox use [<name>] [--clear]` | Set or show the active context |
| `podbox find-definition [<name>]` | Print path to the matching config TOML |
| `podbox list` | List all podbox-managed containers |
| `podbox inspect [<name>]` | Show resolved config, Quadlet, or environment |
| `podbox edit [<name>]` | Open the config in your editor |
| `podbox translate-path --to-container <path>` | Translate a host path to container path |
| `podbox translate-path --to-host <path>` | Translate a container path to host path |
| `podbox completions <shell>` | Generate shell completions |

All commands support `--dry-run` to preview without side effects.

---

## Next Steps

- [Configuration Reference](config.md) — all TOML keys, defaults, and examples
- [Architecture Overview](architecture.md) — how podbox works end-to-end
- [Desktop Integration](export.md) — exporting apps and binaries
- [Troubleshooting Guide](troubleshooting.md) — common issues and fixes
