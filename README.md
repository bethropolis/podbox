<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/podbox-logo.svg">
    <img src="docs/assets/podbox-logo.svg" alt="podbox">
  </picture>
</p>

<p align="center">
  <a href="https://github.com/bethropolis/podbox/releases"><img src="https://img.shields.io/github/v/tag/bethropolis/podbox?label=Version&style=for-the-badge&logo=github&color=3b82f6&labelColor=1e293b&logoColor=white" alt="Version"></a>
  <a href="https://github.com/bethropolis/podbox/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/bethropolis/podbox/ci.yml?label=CI&style=for-the-badge&logo=githubactions&labelColor=1e293b&logoColor=white" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-8b5cf6?style=for-the-badge&logo=opensourceinitiative&labelColor=1e293b&logoColor=white" alt="License"></a>
  <img src="https://img.shields.io/badge/Platform-Linux-6e40c9?style=for-the-badge&logoColor=white&labelColor=1e293b" alt="Platform">
</p>

<p align="center">
  <em>Define once. Run anywhere. No daemon.</em>
</p>

## Key features

- **Declarative TOML** — one file defines the image, packages, config, and lifecycle
- **Isolated home** — never mounts your host home; opt-in directory sharing
- **systemd-managed** — Quadlet units for autostart, restart, and socket activation
- **Guest integration** — notifications, clipboard, URI opening, host commands
- **GPU / Wayland / audio** — auto-detected, opt-out integration

## Why podbox?

Most desktop container tools make a trade-off: full integration means mounting your entire home directory into the container. podbox doesn't. You declare exactly what the container can see — directories, devices, and services — and nothing else is shared.

| | podbox | Distrobox / Toolbox | Raw `podman run` |
|---|---|---|---|
| **Home directory** | Isolated volume, opt-in sharing | Full `$HOME` mounted by default | Manual `-v` flags |
| **Config** | Declarative TOML, version-controllable | Imperative CLI flags | Shell flags per run |
| **Lifecycle** | systemd Quadlet units | Shell shims | Manual |
| **D-Bus** | Filtered via `xdg-dbus-proxy` | Unfiltered session bus | Unfiltered |
| **Wayland / audio** | Opt-out (on by default) | Always on | Manual |
| **GPU** | `auto` / `nvidia` / off | `--nvidia` flag | Manual device flags |
| **Notifications** | Guest interceptor → host | Via shared D-Bus | Not supported |
| **Clipboard** | Guest interceptor → host | Via shared home | Not supported |
| **Host commands** | `host-exec` interceptor | `distrobox-host-exec` | Not supported |
| **SSH agent** | Socket forward (opt-in) | Auto-mounted | Not supported |
| **Baked images** | Yes — packages in image, not runtime | No — packages reinstalled on rebuild | N/A |
| **Reproducibility** | Full — TOML → image → unit | Partial — image only | None |
| **Runtime** | Podman only | Podman / Docker / lilipod | Any OCI runtime |

> podbox is not a distrobox replacement. Distrobox optimises for maximum host integration and is excellent at that. podbox optimises for declared, reproducible environments where you control exactly what is shared.

## Quick Start

```bash
# Install via pre-built binary
curl -fsSL https://bethropolis.github.io/podbox/install.sh | sh

# Create and enter a Fedora container
podbox create fedora
podbox enter fedora
```

See the [Getting Started Guide](docs/getting-started.md) for prebuilt and custom build workflows.

## How It Works

A single TOML definition is your single source of truth. `podbox build` processes it into OCI images and systemd Quadlet units — no manual Containerfile or systemd editing.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/architecture.svg">
    <img src="docs/assets/architecture.svg" alt="podbox architecture" width="100%" style="max-width: 820px;">
  </picture>
</p>

---
## Configuration

Config files live in `~/.config/podbox/<name>.toml` or `./.podbox.toml`. See [the config reference](docs/config.md) for all keys.

## Usage

**Prebuilt (quick):**
```bash
podbox create cachy                
podbox create fedora --name dev 
```

**Custom build (from a base image):**
```bash
# Scaffold a non-prebuilt config
podbox init fedora:44 --name myenv

# Build, enable, start
podbox create myenv
```

**One-shot with any OCI image:**
```bash
podbox create ubuntu:24.04 --name dev
podbox create ghcr.io/user/img --name myenv
```

**Interactive wizard:**
```bash
podbox init -i
```

**Active context — set once, then bare commands work:**

```bash
# Set myenv as the default target
podbox use myenv

# All commands now target myenv
podbox status
podbox logs
podbox exec -- htop
```

**Run things:**

```bash
podbox enter myenv
podbox exec -- htop
podbox run firefox
```

**Export to your host:**

```bash
podbox export app firefox
podbox export bin rg
```

**Manage state:**

```bash
podbox snapshot myenv
podbox restore myenv <tag>
podbox clone work dev
```

**Inspect:**

```bash
podbox inspect myenv
podbox inspect myenv --quadlet
```

## Install

**Online (pre-built binary):**

```bash
curl -fsSL https://bethropolis.github.io/podbox/install.sh | sh
```

**AUR (Arch Linux):**

```bash
paru -S podbox-bin
```

**Local source build:**

```bash
# Install to ~/.local/bin
scripts/install.sh

# Install system-wide (requires sudo)
scripts/install.sh --system
```

## Requirements

### Required

- **Podman** ≥ 5.5 (SSH agent passthrough needs ≥ 5.6)
- **systemd** — user session
- **Linux** with Wayland (X11 apps run via Xwayland)

### Nice to have

- `xdg-dbus-proxy` — needed for filtered D-Bus access (commonly shipped with Flatpak)

## Troubleshooting

Run `podbox doctor` first — it checks the most common issues automatically.

For details on specific issues, see the [Troubleshooting Guide](docs/troubleshooting.md).

> All commands support `--dry-run` to preview without side effects.

## Full Command Reference

See [Commands at a Glance](docs/getting-started.md#commands-at-a-glance) and the [Quick Reference](docs/index.md#quick-reference).

Contributions welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). MIT license.
