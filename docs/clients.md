---
description: Build clients on podbox — the CLI as a stable contract: name handling, JSON output, errors, exit codes, streaming, and what is proposed next.
---

# Building clients

podbox has no daemon, so there is no API to call. The CLI is the API. A bar
widget, a TUI, a GTK app or a script all talk to podbox the same way: run a
command, read its output, check its exit code.

This page is the contract those clients can rely on. Each item is marked:

| Tag | Meaning |
|-----|---------|
| **Today** | Works in the current release. |
| **Proposed** | Not implemented yet. Clients must tolerate it being absent. |

!!! warning ""
    Anything not listed here is not part of the contract and may change
    between releases, including human-readable output.

## Ground rules

1. **Always pass the name.** Never depend on the active context (`podbox use`).
   Use a positional `NAME` or `-C NAME`. **Today**
2. **Use argv arrays, never a shell string.** Spawn `podbox` directly with
   an argument list. No `sh -c`, no string interpolation. **Today**
3. **Read JSON, not text.** Use `--output json` wherever it exists. Only
   JSON output is covered by the stability promise. **Today**
4. **Treat unknown fields as ignorable and known-optional fields as
   optional.** Only the fields listed as required below are guaranteed.
   **Today**
5. **Check the exit code first.** Then parse stdout. **Today**
6. **Preview with `--dry-run`.** Every command accepts it. **Today**

## Name resolution

Every command resolves its target in the same order:

1. positional `NAME`
2. `-C NAME`
3. `$PODBOX_CONTAINER`
4. active context
5. the single config in the config directory, or a local `.podbox.toml`

Clients should use 1 or 2.

**Proposed:** one documented name rule, `^[A-Za-z0-9_.-]+$`, enforced by
podbox itself so clients can validate against the same pattern.

## Reading state

All of these print only JSON on stdout when `--output json` is given.

### `podbox list --output json` **Today**

```json
{"schema": 1, "containers": [{"name": "myenv", "status": "running", "autostart": true, "active": false}]}
```

| Field | Required | Notes |
|-------|----------|-------|
| `name` | yes | Environment name. |
| `status` | yes | `running`, `stopped`, `failed` or `unbuilt`. |
| `autostart` | no | Whether the unit starts on login. |
| `active` | no | Whether this is the active context. |

### `podbox status NAME --output json` **Today**

```json
{"schema": 1, "name": "myenv", "status": "running", "installed": true}
```

`installed` says whether Quadlet files exist, which matters for `unbuilt`
environments.

**Proposed:** optional fields so a client needs one call, not four.

| Field | Meaning |
|-------|---------|
| `base_image` | The image the environment is built from, or the prebuilt reference. |
| `started_at` | RFC 3339 timestamp of the last start. |
| `cpu_percent` | Current CPU use. |
| `memory_bytes` | Current memory use. |
| `memory_limit_bytes` | Configured limit, or absent. |
| `pids` | Number of processes. |

### Other readers **Today**

| Command | Output |
|---------|--------|
| `podbox stats NAME --no-stream --output json` | Resource snapshot. Field names are not yet documented. |
| `podbox history [NAME] --output json` | `{"history": [{"timestamp","name","action","detail"}]}` |
| `podbox snapshot list NAME --output json` | `{"snapshots": [{"tag","created","image"}]}` |
| `podbox provision status NAME --output json` | `{"steps": [{"name","state","applied_at"}]}` |
| `podbox inspect NAME --toml` | The resolved config. |

!!! note ""
    `stats` is the least stable reader today. Document its shape before
    clients build on it.

## Acting on an environment

| Action | Command | Status |
|--------|---------|--------|
| Start | `podbox start NAME` | **Today** |
| Stop | `podbox stop NAME` | **Today** |
| Enter a shell | `podbox enter NAME` (starts it if needed) | **Today** |
| Run a command | `podbox exec NAME -- CMD` | **Today** |
| Open the config | `podbox edit NAME` | **Today** |
| Build | `podbox build NAME` | **Today** |
| Remove | `podbox remove NAME` | **Today** |
| Restart | `podbox restart NAME` | **Proposed** |

Until `restart` exists, clients can run
`systemctl --user restart NAME.service`.

### Interactive commands

`enter`, `exec`, `edit`, `logs -f` and `init -i` need a terminal. A GUI client
should not embed one. Launch the user's own terminal and run the command in
it.

### Mutating commands in JSON mode

**Proposed:** `start`, `stop`, `create`, `build` and `export` accept
`--output json` and print the resulting state:

```json
{"ok": true, "name": "myenv", "status": "running"}
```

They never prompt when `--output json` or `--yes` is given.

## Exports

| Action | Command | Status |
|--------|---------|--------|
| Export an app | `podbox export app NAME ITEM` | **Today** |
| Export a command | `podbox export bin NAME ITEM` | **Today** |
| Remove everything exported | `podbox export clean` with `-C NAME` | **Today** |
| List exportable items and what is exported | `podbox export list NAME --output json` | **Proposed** |
| Remove one item | `podbox export remove app\|bin NAME ITEM` | **Proposed** |

App export reads the `.desktop` file with `podman exec`, so the environment
must be running.

Proposed shape for `export list`:

```json
{"apps": [{"id": "gedit", "name": "Text Editor", "exported": true}],
 "bins": [{"name": "rg", "path": "/usr/bin/rg", "exported": true}]}
```

## Creating and configuring

These exist only as interactive flows or as hand-edited TOML today. A GUI
should not have to drive a wizard or parse TOML.

**Proposed:**

- `podbox create PROFILE --name NAME --yes` with no prompts.
- `podbox config get NAME KEY` and `podbox config set NAME KEY VALUE`, built on
  `toml_edit` so comments and formatting survive. Setting a key that changes
  the image reports that a rebuild is needed.
- `podbox config validate NAME --output json`, returning errors with line
  numbers.

## Errors and exit codes

Exit codes are part of the contract. **Today**

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Any other error |
| 2 | Definition file missing or unreadable |
| 3 | Container or config not found |
| 4 | Build failure or inspect failure |
| 5 | podman not installed |
| 6 | Image pull or tag failure |

Data goes to stdout. Diagnostics go to stderr. A client should show stderr to
the user, and should not parse it.

In JSON mode, failures also print a structured error on stdout with the
same exit code. **Today** (except argument-parse errors, which clap reports
on stderr with exit code 2):

```json
{"schema": 1, "error": {"code": "container_missing", "message": "No environment named 'dev'."}}
```

Codes are stable lowercase identifiers. Messages are for people.

Two readers print their full JSON report and then fail only for the exit
status: `doctor` with failing checks (`checks_failed`) and `provision
status --check` with pending steps (`steps_pending`). Those print no second
envelope, so stdout stays a single JSON document — read the report.

## Staying up to date without polling

A client that wants live state has two options.

1. **Poll `list`.** Every 5 to 10 seconds is enough for a status indicator.
   Never run two overlapping polls of the same command. **Today**
2. **Subscribe.** **Proposed:** `podbox watch --output ndjson` runs in the
   foreground and prints one JSON line per state change:

   ```json
   {"event": "status", "name": "myenv", "status": "running"}
   ```

   The client spawns it as a child process and stops it when it is done.
   Nothing runs in the background, so the no-daemon promise holds.

Builds can be slow. **Proposed:** `podbox build NAME --output ndjson` prints
progress events (`{"event": "step", "index": 3, "total": 9, "text": "..."}`),
ending with a `{"event": "done", "ok": true}` line.

## Version and capability checks

**Proposed:** `podbox info --output json`:

```json
{"podbox": "0.8.2", "podman": "5.6.0", "schema": 1, "features": ["export_list", "watch"]}
```

Every podbox-owned JSON payload carries `"schema": 1`. **Today** The number
changes only on a breaking change, and clients should refuse to run against
a higher schema than they know. New fields and new commands never bump it.

Exception: `stats --output json` is a raw `podman stats` passthrough with
no envelope, and stays that way until its shape is documented.

## Safety

- Validate environment and item names before passing them to any command.
- Treat all text from podbox and from container files as untrusted. Do not
  render it as markup.
- Time out reads after a few seconds and show a quiet error instead of
  freezing the interface.
- If `podbox` is not installed (exit code 5, or not found on `PATH`), do not
  retry on every poll. Check again when the user opens the client.

## Testing against the contract

**Proposed:** podbox keeps golden JSON fixtures in `tests/fixtures/` for every
reader above, and CI fails when output changes shape. Clients can copy these
fixtures to build their own tests without a running container.

## Checklist for maintainers

In priority order:

1. Document this page's **Today** items and the `stats` shape.
2. Extend `status --output json` with the optional fields.
3. Add `export list --output json` and per-item removal.
4. Add `restart`.
5. ~~Add `"schema": 1` to every JSON payload and the structured error format.~~ Done.
6. Add `info`, then `config get|set|validate`, then non-interactive `create`.
7. Add `watch` and `build --output ndjson`.
8. Add the golden fixtures.
