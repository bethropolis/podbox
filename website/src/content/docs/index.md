# Documentation

## User Guides

| Doc | What it covers |
|-----|---------------|
| [Quick Start](getting-started.md) | Install, create a container, essential workflows |
| [Configuration Reference](config.md) | All TOML keys, defaults, and examples |
| [Base packages](baked-in-packages.md) | Tools, shells, locales preinstalled in custom builds |
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

## Quick Reference

```bash
podbox use <name>                # set active context
podbox profile list              # list available profiles
podbox profile show <name>       # show profile TOML
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
See [cli.md](cli.md) for name resolution, exit codes, and JSON output.
