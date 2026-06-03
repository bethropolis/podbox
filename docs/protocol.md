# Host-Guest Socket Protocol

## Wire Format

Length-prefixed JSON over a Unix stream socket:

```
┌────────────────────────┬─────────────────────────────┐
│ 4 bytes (big-endian u32│ N bytes (UTF-8 JSON)        │
│ payload length = N)    │                             │
└────────────────────────┴─────────────────────────────┘
```

## Socket Location

- **Host socket:** `$XDG_RUNTIME_DIR/podbox/<name>.sock`
  Created by the `.socket` Quadlet unit.
- **Local guest socket:** `/run/podbox/guest-<name>.sock`
  Created by `podbox-guest --daemon` for interceptors.

## Handshake

Guest sends:
```json
{"type": "hello", "version": "0.1.0", "container": "myenv", "capabilities": ["notify", "xdg_open", "clipboard", "host_exec"]}
```

Host responds:
```json
{"type": "hello_ack", "accepted": ["notify", "xdg_open"], "rejected": ["clipboard", "host_exec"]}
```

## Message Types

### Guest → Host

| type | fields |
|------|--------|
| `hello` | `version`, `container`, `capabilities` |
| `notify` | `summary`, `body`, `urgency`, `actions` (optional), `app_name` (optional) |
| `notify_action_result` | `key` |
| `xdg_open` | `uri` |
| `clipboard_set` | `text` |
| `clipboard_get` | — |
| `host_exec` | `command` |
| `host_exec_stdout` | `data` |
| `host_exec_stderr` | `data` |
| `host_exec_done` | `exit_code` |

### Host → Guest

| type | fields |
|------|--------|
| `hello_ack` | `accepted`, `rejected` |
| `clipboard_data` | `text` |
| `ping` | — |
| `shutdown` | — |

### `notify` actions field

When present, `actions` is an array of `{ "key": "...", "label": "..." }` objects.
The guest sends `notify_action_result` with the selected `key` back to the host.

### Capabilities

| Capability | Interceptor | Description |
|------------|-------------|-------------|
| `notify` | `notify-send` | Desktop notification forwarding |
| `xdg_open` | `xdg-open` | URI opening via host |
| `clipboard` | `podbox-clipboard` | Clipboard sharing |
| `host_exec` | `host-exec` | Execute commands on host |
