# podbox-cli

A declarative container environment manager for Linux, built on Podman and systemd Quadlet. Define an environment as a single TOML file; podbox turns it into an OCI image and the systemd units that own its lifecycle. No podbox daemon runs in the background.

Think distrobox, but the environment is a file you can commit to git instead of a sequence of flags you ran once and forgot.

GUI-capable dev containers with Wayland, D-Bus, clipboard, notifications, and host-command integration, all routed through a small guest interceptor.

## Install

```bash
cargo install podbox-cli
```

> The crates.io build supports **prebuilt images only** (`image_ref` in your
> config, or `podbox create ghcr.io/bethropolis/podbox:<tag>`). Custom image
> builds (a `[image] base = "..."` in the TOML) need a full source build from
> the workspace, where the guest daemon is embedded at compile time.

## Quick start

```bash
podbox create cachy          # prebuilt environment, ready in seconds
podbox enter cachy           # hop in
podbox export app firefox    # pull apps and binaries out to the host
```

Building from a base image instead:

```bash
podbox init fedora:44 --name myenv
podbox create myenv
```

## Requirements

- Podman ≥ 5.5
- A systemd user session
- Linux with a Wayland compositor

Run `podbox doctor` to check your setup, and `podbox --help` for the full command list.

See the [source repository](https://github.com/bethropolis/podbox) for the getting-started guide, configuration reference, and troubleshooting.
