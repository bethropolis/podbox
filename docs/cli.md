---
description: podbox CLI reference — every command grouped by workflow, container name resolution, exit codes, JSON output, and shell completion.
---

# CLI reference

## Command groups

| Group | Commands |
|-------|----------|
| Get started | `create`, `init`, `profile` |
| Day to day | `enter` (alias `shell`), `exec`, `run`, `start`, `stop`, `list` (alias `ls`), `status` |
| Change | `edit`, `build`, `enable`, `disable`, `update`, `rollback`, `pull`, `diff` |
| Inspect | `logs`, `inspect`, `stats`, `doctor`, `history`, `find-definition` |
| Copy / backup | `clone`, `snapshot`, `restore`, `export` |
| Remove | `remove` (alias `rm`) |
| Context | `use` |
| Dotfiles | `dotfiles sync`, `dotfiles status` |
| Storage | `cache list`, `cache prune` |

`serve`, `compositor`, `__complete-names`, and `internal-stdin-watchdog`
are hidden systemd plumbing — callable, but not for daily use.

## Naming a container

Every command resolves its target the same way:

1. positional `NAME`
2. `-C NAME`
3. `$PODBOX_CONTAINER`
4. active context (`podbox use`)
5. single config in the config dir / local `.podbox.toml`

A leading name works podman-style too (`podbox exec myenv ls`), but only
for a known config with more arguments after it. Explicit `-C` always wins.

| Flag | What it does |
|------|--------------|
| `--here` (`enter`, `exec`) | Start in the container-side twin of your current dir. Only for already-mounted paths — otherwise the error tells you which mount to add. Never sneaks a mount into a running container. |
| `-e KEY=VALUE` (repeatable; `enter`, `exec`, `run`) | Override env. Host vars cross over only when `[container.env].forward` names them; explicit `-e` wins over both. |

Two settings have no per-command override:

- `network.offline = true` is container-wide and persistent.
- With `[lifecycle].auto_checkpoint = true`, `update` and `build --rebuild`
  tag the current image as `checkpoint-prev` first; `podbox rollback [NAME]`
  points the Quadlet back at it. Home dir and config untouched.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | success |
| 2 | definition file missing / unreadable (includes `find-definition NAME` miss) |
| 3 | container/config not found for the requested operation |
| 4 | build failure or podman inspect failure |
| 5 | podman not installed |
| 6 | image pull/tag failure |
| 1 | anything else |

## JSON output

Read commands take `--output json` and print nothing else on stdout:

- `list`: `{"containers": [{"name","status","autostart","active"}]}`
- `status`: `{"name","status","installed"}`
- `snapshot list`: `{"snapshots": [{"tag","created","image"}]}`
- `history`: `{"history": [{"timestamp","name","action","detail"}]}`

Status values: `running | stopped | failed | unbuilt`. `installed` says
whether Quadlet files exist for an unbuilt container.

## History

Lifecycle commands log to `~/.local/state/podbox/history.log` on success.
Best-effort — logging never fails the command.

```bash
podbox history              # newest first, all containers (default 25)
podbox history myenv        # one container
podbox history --limit 0    # no limit; --output json for scripting
```

Recorded: `create`, `build`, `enable`, `disable`, `start`, `stop`,
`update`, `remove`, `recover`.

## Shell completion

```bash
podbox completions bash > ~/.local/share/bash-completion/completions/podbox
podbox completions zsh  > "${fpath[1]}/_podbox"
podbox completions fish > ~/.config/fish/completions/podbox.fish
```

Names complete after name-taking subcommands and as `-C` values, for
**bash**, **zsh**, and **fish**. No configs, no candidates — completion
never errors.

<details>
<summary>Fish abbreviations</summary>

`podbox completions fish --abbrs` appends opt-in `abbr` shorthand (fish
only — ignored for bash/zsh):

```fish
podbox completions fish --abbrs | source
```

| Token | Expands to | | Token | Expands to |
|-------|------------|--|-------|------------|
| `pb`  | `podbox`   | | `pbs` | `podbox start` |
| `pbb` | `podbox build` | | `pbt` | `podbox stop` |
| `pbc` | `podbox create` | | `pbu` | `podbox update` |
| `pbd` | `podbox doctor` | | `pbv` | `podbox status` |
| `pbe` | `podbox enter` | | `pbx` | `podbox exec --` |
| `pbl` | `podbox list` | | `pbr` | `podbox recover` |

</details>
