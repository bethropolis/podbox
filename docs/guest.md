# Guest Daemon Architecture

The guest daemon (`podmgr-guest`) runs inside the container. It bridges
container capabilities (notifications, URI opening, clipboard) to the host
via a Unix socket connection.

---

## Entry Point (`entry.rs`)

The container starts with `podmgr-guest --entry [<command>...]`.

1. **`fork()`** splits into two processes:

   - **Child** (daemon process): redirects stdio to `/dev/null`, then
     execs `podmgr-guest --daemon` (re-exec). Runs the event loop.

   - **Parent** (shell/command process): if a command was given, execs it
     via `execv`. If empty, execs a login shell (`$SHELL` or `/bin/bash`,
     with `argv[0]` prefixed by `-` for login mode).

2. The parent **replaces itself** with the shell/command. The child runs
   independently as a background daemon with a 5-minute idle timeout.

---

## Daemon Lifecycle (`daemon.rs`)

### Startup sequence

1. **Create `/run/podmgr/bin/`** — directory for interceptor symlinks
2. **Connect to host socket** — `$XDG_RUNTIME_DIR/podmgr/<container>.sock`
   with poll-based retry (3 attempts, 500ms interval, zero CPU)
3. **Handshake** — sends capability list (`notify`, `xdg_open`, `clipboard`)
   to host; host responds with accepted subset
4. **Install interceptors** — creates symlinks in `/run/podmgr/bin/` for
   each accepted capability
5. **PATH injection** — writes `/etc/environment.d/podmgr.conf` that
   prepends `/run/podmgr/bin` to `PATH`
6. **Event loop** — polls the host socket for messages

### Event loop

The event loop is `poll()`-based on a single file descriptor (the host
socket connection). It uses a **5-minute idle timeout** — if no message
arrives, the daemon exits gracefully.

| Event | Action |
|-------|--------|
| `Shutdown` message | Exit daemon |
| `Ping` message | No-op (keepalive) |
| `None` / EOF | Host disconnected; exit |
| `POLLHUP` / `POLLERR` | Host socket hung up; exit |
| Idle timeout (5 min) | No messages received; exit |
| `EINTR` | Retry `poll()` |

The daemon consumes **0% CPU** when idle — it is parked in the kernel by
`poll()`.

---

## Socket Protocol

The daemon connects to the host socket at
`$XDG_RUNTIME_DIR/podmgr/<container>.sock`. Messages are length-prefixed
JSON over a Unix stream socket (see [protocol.md](protocol.md) for the wire
format).

### Handshake

```
→ {"type":"hello","version":"0.1.0","container":"myenv","capabilities":["notify","xdg_open","clipboard"]}
← {"type":"hello_ack","accepted":["notify","xdg_open"],"rejected":["clipboard"]}
```

---

## Interceptors

### Symlink dispatch

The daemon creates symlinks in `/run/podmgr/bin/` pointing to the
`podmgr-guest` binary:

| Symlink | Target | Capability |
|---------|--------|------------|
| `/run/podmgr/bin/notify-send` | `podmgr-guest` | `notify` |
| `/run/podmgr/bin/xdg-open` | `podmgr-guest` | `xdg_open` |
| `/run/podmgr/bin/podmgr-clipboard` | `podmgr-guest` | `clipboard` |

The binary detects which name was used to invoke it via `argv[0]` and
dispatches to the appropriate interceptor module (`main.rs`).

### PATH injection

`/etc/environment.d/podmgr.conf` is written with:

```
PATH=/run/podmgr/bin:$PATH
```

This ensures the interceptor symlinks take precedence over system-installed
binaries.

### Interceptor types

| Interceptor | File | What it does |
|-------------|------|-------------|
| `notify-send` | `interceptors/notify.rs` | Parses CLI args, sends `GuestMessage::Notify` to host |
| `xdg-open` | `interceptors/xdg_open.rs` | Sends URI in `GuestMessage::XdgOpen` to host |
| `podmgr-clipboard` | `interceptors/clipboard.rs` | `set`: reads stdin, sends `ClipboardSet`; `get`: sends `ClipboardGet`, writes response to stdout |

Each interceptor opens a **direct, ephemeral** Unix socket connection to
the host socket (not the daemon's persistent connection), sends its
message, and waits for acknowledgement before exiting.
