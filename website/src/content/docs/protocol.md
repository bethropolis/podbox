---
description: podbox host-guest Unix socket protocol — wire format, handshake, message types, capability negotiation, and notify action callbacks.
---

# Host-Guest Socket Protocol

## Wire Format

Length-prefixed JSON over a Unix stream socket:

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/wire_format.svg">
    <img src="assets/wire_format.svg" alt="Message wire format" width="100%" style="max-width: 820px;">
  </picture>
</p>

## Socket Location

| Socket | Path | Created by |
|--------|------|------------|
| Host socket | `$XDG_RUNTIME_DIR/podbox/<name>.sock` | `.socket` Quadlet unit |
| Local guest socket | `/run/podbox/guest-<name>.sock` | `podbox-guest --daemon` |

The host socket is created by systemd before the container starts and persists across restarts. The guest socket is used by interceptor processes to communicate with the local daemon.

## Handshake

**Guest sends:**

```json
{
  "type": "hello",
  "version": "0.1.0",
  "container": "myenv",
  "capabilities": ["notify", "xdg_open", "clipboard", "host_exec"]
}
```

**Host responds:**

```json
{
  "type": "hello_ack",
  "accepted": ["notify", "xdg_open"],
  "rejected": ["clipboard", "host_exec"],
  "idle_timeout_secs": 0
}
```

The handshake decides which capabilities the guest may use
(`0` timeout = disabled). The guest only installs interceptor symlinks for
accepted ones.

## Message Types

### Guest → Host

| Type | Fields |
|------|--------|
| `hello` | `protocol_version`, `guest_version`, `container`, `capabilities` |
| `notify` | `summary`, `body`, `urgency`, `actions` (optional), `app_name` (optional) |
| `xdg_open` | `uri` |
| `clipboard_set` | `text` (at most 8 MiB; larger content gets an `error` reply and leaves the session open) |
| `clipboard_get` | — (the host answers `error` when the selection exceeds 8 MiB) |
| `host_exec` | `cmd`, `args` |
| `host_exec_v2` | `cmd`, `args`, `env` (optional key/value pairs; only `TERM`, `LANG`, `COLORTERM` pass), `cwd` (optional guest working directory; interpreted per the configured [cwd handling](config.md#cwd-handling), defaulting to the host `$HOME`), `tty` (optional; see [TTY sessions](#tty-sessions) below) — the child's stdin/stdout/stderr arrive as three descriptors via `SCM_RIGHTS` on the same connection |
| `host_exec_signal` | `sig` (optional signal number to relay to a running fd-passing host-exec child) |
| `register_session` | — (pidfd via `SCM_RIGHTS`) |
| `busy` | — |
| `idle_timeout` | — |

### Host → Guest

| Type | Fields |
|------|--------|
| `hello_ack` | `accepted`, `rejected`, `idle_timeout_secs` |
| `clipboard_data` | `text` |
| `host_exec_stdout` | `data` |
| `host_exec_stderr` | `data` |
| `host_exec_output` | `stream` (`stdout`/`stderr`), `data` (raw bytes) — chunked streaming output, only when the guest negotiated `host_exec_stream` |
| `host_exec_done` | `exit_code`, `signal` (optional; the killing signal when the child died of one — `exit_code` is then `128 + signal`) |
| `notify_action_result` | `notification_id`, `action_key` |
| `ping` | — |
| `check_idle` | — |
| `shutdown` | — |

## Notify actions

`actions` is an optional array of `{key, label}`. The host replies with
`notify_action_result` carrying the `notification_id` and the chosen
`action_key`.

```json
{
  "type": "notify",
  "summary": "Build complete",
  "body": "Exit code: 0",
  "actions": [
    { "key": "open", "label": "Open project" },
    { "key": "dismiss", "label": "Dismiss" }
  ]
}
```

Older guests omit `actions`/`app_name` — both default empty server-side.

## Capabilities

One interceptor symlink per capability. Rejected ones are skipped silently —
no symlink, no retries.

| Capability | Interceptor | Description |
|------------|-------------|-------------|
| `notify` | `notify-send` | Desktop notification forwarding |
| `xdg_open` | `xdg-open` | URI opening via host |
| `clipboard` | `podbox-clipboard` | Clipboard sharing |
| `host_exec` | `host-exec` | Execute commands on host |
| `host_exec_stream` | — (requested by `host-exec` alongside `host_exec`) | Chunked raw-byte output streaming; without it the host buffers one frame per pipe |
| `host_exec_fds` | — (requested by `host-exec` alongside `host_exec`) | Attached-stdio sessions: `host_exec_v2` requests carry the child's three stdio descriptors via `SCM_RIGHTS`, so bytes flow through the pipes themselves with no protocol copying; `host_exec_signal` relays signals to the child |

A guest that predates `host_exec_stream` never requests it and keeps
receiving buffered `host_exec_stdout`/`host_exec_stderr` frames, and it
parses the newer `host_exec_done` fine (the extra `signal` field is
ignored). No protocol version bump was needed.

`host_exec_fds` follows the same rule. A guest without it keeps using the
plain `host_exec` frames exactly as before; a host that predates the
capability never grants it, so an acknowledging guest falls back to the
plain path. The child spawned for an fd-passing session sits in its own
process group, so Ctrl-C typed in the guest's terminal reaches it as a
relayed SIGINT rather than killing anything else; the exit is still
reported through the regular `host_exec_done` frame.

## TTY sessions

A fd-passing `host_exec_v2` request with `tty = true` carries the slave
end of the guest's own pty three times (one descriptor per stdio slot).
The guest allocates the pty when its stdout is a terminal, copies its
terminal's termios and window size onto the slave, and keeps the master
open until the session's exit report arrives — dropping it hangs the
terminal up.

The host runs the child as the new session's leader: `setsid` gives it
a process group of its own (the same group `host_exec_signal` targets),
and `TIOCSCTTY` on the inherited stdin makes the pty its controlling
terminal, so the kernel delivers Ctrl-C, job control and window-size
changes on the child directly, the way an interactive shell expects.
An error in that setup aborts the spawn, and the guest sees the same
typed failure a missing command does.

Everything else on the session is unchanged: the cwd policy, the
allowlist and argument filter apply the same way, output flows through
the descriptors rather than the protocol, and the exit arrives as one
`host_exec_done`. The per-entry `timeout` (below) never applies to a
tty session: an interactive child outliving its request is correct
behaviour, not a leak. Window-size changes are one-way: the session
starts with the geometry the guest's terminal had, and later resizes
by either side are not propagated.