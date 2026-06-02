# Documentation

## User Guides

| Doc | What it covers |
|-----|---------------|
| [Quick Start](../README.md) | Install, create a container, essential workflows |
| [Configuration Reference](config.md) | All TOML keys, defaults, and examples |
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
| [Project Guide](../PROJECT_GUIDE.md) | Full module specs, testing strategy, debugging |
| [Agent Instructions](../AGENT.md) | Rules and checklists for AI-assisted development |
| [Roadmap](../ROADMAP.md) | Phase plans and scope |

## Quick Reference

```bash
podbox create <profile>        # create + build + enable + start
podbox shell                   # open a shell
podbox run <app>               # run a GUI app
podbox doctor --fix            # fix common issues
podbox export app <name>       # add to host launcher
podbox remove --all            # full cleanup
```

See the [README](../README.md) for the full command reference.
