# Documentation

## User Guides

| Doc | What it covers |
|-----|---------------|
| [Quick Start](../README.md) | Install, create a container, essential workflows |
| [Configuration Reference](config.md) | All TOML keys, defaults, and examples |
| [Baked-in Base Packages](baked-in-packages.md) | Auto-installed packages, locale, timezone, sudo |
| [Desktop Integration](export.md) | Exporting container apps and binaries to the host |
| [Container Integration](guest.md) | How the guest daemon bridges notifications, URI opening, clipboard |
| [D-Bus Proxy](dbus-proxy.md) | Filtered D-Bus access via xdg-dbus-proxy |

## Reference

| Doc | What it covers |
|-----|---------------|
| [Architecture Overview](architecture.md) | How podbox works end-to-end |
| [Quadlet Keys](quadlet.md) | Generated systemd unit files |
| [Host-Guest Protocol](protocol.md) | Wire format and message types |
| [Exit Codes](architecture.md#exit-codes) | Program exit code meanings |

## Developer

| Doc | What it covers |
|-----|---------------|
| [Roadmap](../ROADMAP.md) | Phase plans and scope |
| [PLAN.md](../PLAN.md) | Implementation plans |

## Quick Reference

```bash
podbox use <name>                # set active context
podbox create <profile>          # create + build + enable + start
podbox enter <name>              # open a shell
podbox exec -- <cmd>             # run a command
podbox run <app>                 # run a GUI app
podbox stats                     # show resource usage
podbox doctor --fix              # fix common issues
podbox export app <name>         # add to host launcher
podbox diff                      # check installed packages vs config
podbox remove --all              # full cleanup
```

Most commands accept an optional `<name>` — defaults to the active context.
See the [README](../README.md) for the full command reference.
