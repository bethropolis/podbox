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
  <em>Define once. Run anywhere. No host daemon.</em>
</p>

podbox is a declarative container environment manager for Linux. You write a TOML file, podbox turns it into an OCI image and a set of systemd Quadlet units, and from then on systemd owns the lifecycle: autostart, restart, socket activation, all of it. No background service of podbox's own running on your machine.

Think distrobox, but the environment is a file you can commit to git instead of a sequence of flags you ran once and forgot.

## What it does

Each environment is one TOML file: image, packages, config, and how it should run. `podbox build` turns that into an OCI image and the matching systemd Quadlet units, nothing hand-edited.

The container gets its own home directory by default. Extra folders, devices, GPU, and the Wayland socket are all opt-in. Notifications, clipboard, opening links, and running a command on the host all work, routed through a small guest interceptor rather than a raw bind mount.

systemd owns the lifecycle from there: autostart, restarts, socket activation. There's no podbox process running in the background.

## vs distrobox

Distrobox mounts your home directory and session bus by default and gets out of the way after that, which is the right approach if you want a container that feels like the host. podbox defaults to the opposite: nothing is shared unless it's in the TOML, and the environment is reproducible from that file rather than whatever state the container happened to drift into.

It's not a replacement for distrobox, it solves a different problem.

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
| **Baked images** | Yes: packages in image, not runtime | No: packages reinstalled on rebuild | N/A |
| **Reproducibility** | Full: TOML → image → unit | Partial: image only | None |
| **Runtime** | Podman only | Podman / Docker / lilipod | Any OCI runtime |

## Quick start

```bash
# Grab the binary
curl -fsSL https://bethropolis.github.io/podbox/install.sh | sh

# Spin up a Fedora container and hop in
podbox create fedora
podbox enter fedora
```

That's a prebuilt environment with no config file needed. For anything custom, see the [Getting Started Guide](docs/getting-started.md).

## How it works

You write one TOML file. `podbox build` turns it into an OCI image plus the systemd Quadlet units that run it: no hand-edited Containerfile, no manually written unit files.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/architecture.svg">
    <img src="docs/assets/architecture.svg" alt="podbox architecture" width="100%" style="max-width: 820px;">
  </picture>
</p>

---

## Configuration

Configs live in `~/.config/podbox/<name>.toml`, or `./.podbox.toml` if you'd rather keep one per project. Every key is documented in the [config reference](docs/config.md).

## Using it day to day

**Prebuilt environments, ready in seconds:**
```bash
podbox create cachy
podbox create fedora --name dev
```

**Building from a base image instead:**
```bash
# Scaffold a config you can edit
podbox init fedora:44 --name myenv

# Build it, enable it, start it
podbox create myenv
```

**Or just point it at any OCI image:**
```bash
podbox create ubuntu:24.04 --name dev
podbox create ghcr.io/user/img --name myenv
```

**Not sure what you want? There's a wizard:**
```bash
podbox init -i
```

**Tired of typing the env name every time?** Set an active context and bare commands target it:
```bash
podbox use myenv

podbox status
podbox logs
podbox exec -- htop
```

**Getting in and running things:**
```bash
podbox enter myenv
podbox exec -- htop
podbox run firefox
```

**Pulling apps and binaries out to your host:**
```bash
podbox export app firefox
podbox export bin rg
```

**Snapshots, restores, clones:**
```bash
podbox snapshot myenv
podbox restore myenv <tag>
podbox clone work dev
```

**Peeking under the hood:**
```bash
podbox inspect myenv
podbox inspect myenv --quadlet
```

## Installing

**Pre-built binary:**
```bash
curl -fsSL https://bethropolis.github.io/podbox/install.sh | sh
```

**Arch Linux, via AUR:**
```bash
paru -S podbox-bin
```

**Building from source:**
```bash
scripts/install.sh            # installs to ~/.local/bin
scripts/install.sh --system   # system-wide, needs sudo
```

## What you'll need

**Required:**
- Podman ≥ 5.5 (5.6+ if you want SSH agent passthrough)
- A systemd user session
- Linux with a Wayland compositor (X11 apps work via Xwayland)

**Nice to have:**
- `xdg-dbus-proxy`, for filtered D-Bus access. Usually already on your system if you've got Flatpak installed.

## Something not working?

Run `podbox doctor` first. It catches most of the common setup issues on its own. If that doesn't sort it out, the [Troubleshooting Guide](docs/troubleshooting.md) covers specific problems in more depth.

Every command also takes `--dry-run` if you want to see what it'd do before it does it.

## Full command reference

See [Commands at a Glance](docs/getting-started.md#commands-at-a-glance) or the [Quick Reference](docs/index.md#quick-reference) for the complete list.

---

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md). MIT licensed.
