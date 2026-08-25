# CLI reference

Groups, name resolution, exit codes, JSON output, and shell completion.

## Command groups

| Group | Commands |
|-------|----------|
| Get started | `create`, `init`, `profile` |
| Day to day | `enter` (alias `shell`), `exec`, `run`, `start`, `stop`, `list` (alias `ls`), `status` |
| Change | `edit`, `build`, `enable`, `disable`, `update`, `pull`, `diff` |
| Inspect | `logs`, `inspect`, `stats`, `doctor`, `find-definition` |
| Copy / backup | `clone`, `snapshot`, `restore`, `export` |
| Remove | `remove` (alias `rm`) |
| Context | `use` |

Systemd internals (`serve`, `compositor`, `__complete-names`,
`internal-stdin-watchdog`) are hidden but callable; Quadlet units depend on
the first two.

## Naming a container

Every container command resolves its target the same way:

1. positional `NAME`
2. `-C NAME`
3. `$PODBOX_CONTAINER`
4. active context (`podbox use`)
5. single config in the config dir / local `.podbox.toml`

`exec` and `run` also accept a podman-style leading name
(`podbox exec myenv ls`). It is treated as the container only when it matches
a known config **and** more arguments follow, so `podbox exec -- ls` and a
bare `podbox exec fedora` behave as before. An explicit `-C` always wins.

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

Read commands accept `--output json` and print nothing else on stdout:

- `list`: `{"containers": [{"name","status","autostart","active"}]}`
- `status`: `{"name","status","installed"}`
- `snapshot list`: `{"snapshots": [{"tag","created","image"}]}`

`status` vocabulary is shared with `list`:
`running | stopped | failed | unbuilt`. The extra boolean `installed` reports
whether Quadlet files exist for an unbuilt container (formerly expressed as
"not built" vs "not installed").

## Shell completion

```bash
podbox completions bash > ~/.local/share/bash-completion/completions/podbox
podbox completions zsh  > "${fpath[1]}/_podbox"
podbox completions fish > ~/.config/fish/completions/podbox.fish
```

The generated scripts include dynamic container-name completion (fed by
`podbox __complete-names`, which prints config stems) for **bash** and
**fish**: names complete after name-taking subcommands and as `-C/--container`
values. Missing configs yield no candidates — completion never errors.
