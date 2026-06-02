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
{"type": "hello", "version": "0.1.0", "container": "myenv", "capabilities": ["notify", "xdg_open", "clipboard"]}
```

Host responds:
```json
{"type": "hello_ack", "accepted": ["notify", "xdg_open"], "rejected": ["clipboard"]}
```

## Message Types

### Guest → Host

| type | fields |
|------|--------|
| `hello` | `version`, `container`, `capabilities` |
| `notify` | `summary`, `body`, `urgency` |
| `xdg_open` | `uri` |
| `clipboard_set` | `text` |
| `clipboard_get` | — |

### Host → Guest

| type | fields |
|------|--------|
| `hello_ack` | `accepted`, `rejected` |
| `clipboard_data` | `text` |
| `ping` | — |
| `shutdown` | — |
