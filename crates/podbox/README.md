# podbox-cli

A declarative container environment manager for Linux, built on Podman and systemd Quadlet. Define an environment as a single TOML file; podbox turns it into an OCI image and the systemd units that own its lifecycle. No podbox daemon runs in the background.

Think distrobox, but the environment is a file you can commit to git instead of a sequence of flags you ran once and forgot.

GUI-capable dev containers with Wayland, D-Bus, clipboard, notifications, and host-command integration, all routed through a small guest interceptor.

## Install

```bash
cargo install podbox-cli
```

> The crates.io build works with **prebuilt images** — any image that already ships
> the podman guest, whether that's your own (`podbox create ghcr.io/you/img:<tag>`)
> or the official ones built from the [`images/`](https://github.com/bethropolis/podbox/tree/main/images) directory (`podbox create cachy`). Only fresh builds
> from a bare base (`[image] base`) require a workspace build to embed the guest.

## Quick start

```bash
podbox create cachy          # prebuilt environment, ready in seconds
podbox enter cachy           # hop in
podbox export app firefox    # pull apps and binaries out to the host
```

## Requirements

- Podman ≥ 5.5
- A systemd user session
- Linux with a Wayland compositor

Run `podbox doctor` to check your setup, and `podbox --help` for the full command list.

See the [source repository](https://github.com/bethropolis/podbox) for the getting-started guide, configuration reference, and troubleshooting.
