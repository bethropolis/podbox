---
description: podbox-guest daemon internals — startup sequence, event loop, socket protocol, and interceptors for notifications, clipboard, xdg-open, and host-exec.
---

# Guest daemon

`podbox-guest` runs inside the container. It bridges notifications, URI
opening, clipboard, and host execution to the host over a Unix socket.

## Entry point

The container starts with `podbox-guest --entry [<command>...]`:

1. Symlinks `/run/user/%U/flatpak-info` → `/.flatpak-info`, so
   portal-aware toolkits route audio/video capture through portals.
2. `fork()`s: the child re-execs `podbox-guest --daemon`; the parent execs
   your command (or a login shell).
3. The daemon idles in the background with a 5-minute timeout.

## Daemon startup

1. Creates `/run/podbox/bin/` for interceptor symlinks.
2. Compares `PODBOX_HOST_VERSION` against its own version; warns on drift.
3. Connects to the host socket (3 tries, 500ms apart).
4. Handshakes — sends its capabilities, gets back the accepted subset.
5. Symlinks one interceptor per accepted capability.
6. Prepends `/run/podbox/bin` to `PATH` via `/etc/profile.d/podbox.sh`
   and `/etc/fish/conf.d/podbox.fish`.
7. Enters the event loop: `poll()` on the socket, 0% CPU when idle.
   With `lifecycle.idle_timeout` set, it exits once no user processes
   remain past the timeout.

### Event loop

| Event | Action |
|-------|--------|
| `Shutdown` | Exit |
| `Ping` | No-op (keepalive) |
| `CheckIdle` | Scan `/proc`; reply `Busy` or `IdleTimeout` |
| Disconnect / `POLLHUP` / `POLLERR` | Exit |
| Idle timeout expired | Send `IdleTimeout`, exit |
| `EINTR` | Retry `poll()` |

User processes are tracked via pidfds (Linux 5.3+). 0% CPU when idle.

## Socket protocol

Length-prefixed JSON over `$XDG_RUNTIME_DIR/podbox/<container>.sock`.
Full wire format: [protocol.md](protocol.md).

```
→ {"type":"hello","version":"0.1.0","container":"myenv","capabilities":["notify","xdg_open","clipboard","host_exec"]}
← {"type":"hello_ack","accepted":["notify","xdg_open"],"rejected":["clipboard","host_exec"]}
```

## Interceptors

### Symlink dispatch

Symlinks in `/run/podbox/bin/`, one per accepted capability. The binary
reads `argv[0]` to know which interceptor it is. They shadow system
binaries via `PATH` (`/etc/profile.d/podbox.sh`, `/etc/fish/conf.d/podbox.fish`).

| Symlink | Capability | What it does |
|---------|------------|-------------|
| `notify-send` | `notify` | Forwards args; `--action`/`-A` buttons wait for the host's reply |
| `xdg-open` | `xdg_open` | Sends the URI to the host |
| `podbox-clipboard` | `clipboard` | `set` reads stdin; `get` writes the host clipboard to stdout |
| `host-exec` | `host_exec` | Runs the command on the host, relays output, exits with its code |

Each opens its own short-lived socket connection, sends one message, waits
for the reply, exits.

### Host-exec security

Off by default. When enabled, the host validates every command:

| Check | Rejected | Example error |
|-------|----------|---------------|
| Allowlist | Anything not in the map (guest `$PATH` ignored) | `Permission denied: 'ls' is not in the host-exec allowlist` |
| Shell metacharacters | `;`, `\|`, `&`, `$`, `` ` `` in args | `host-exec: failed to execute 'echo $HOME'` |
| Dangerous flags | `--exec-path`, `--config`, `-o`, … | `Security violation: argument "--exec-path=/tmp/x" …` |
| Absolute-path bypass | `/usr/bin/git` when the key is `git` | `Permission denied: '/usr/bin/git' is not in the host-exec allowlist` |

See [config.md](config.md#integrationhost_exec) for the allowlist shape —
and prefer wrapper scripts over general-purpose tools.
