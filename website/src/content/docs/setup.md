---
title: Home Setup
description: One-shot setup inside a podbox container home — dotfiles and provisioning steps, both stamped, plus host-side automation.
---

The image is baked once. The home is a persistent volume that follows you
around. Home setup is the bridge: the things you want to happen *inside the
container, once per home*, without rebuilding the image.

| Use this | For |
|----------|-----|
| `[dotfiles]` | Copying or cloning your config files into the home |
| `[[provision]]` | Ordered setup commands — toolchains, config defaults, repo clones |
| `image.run.commands` | Baked into the image; can never reach the home |
| `[container.services]` | Long-running processes |

Both run during `create`, after the container starts. Both are optional, both
fail soft, and neither ever runs on a plain `start`, `enter`, or `exec`.

## `[dotfiles]`

Populate the container home from a directory on the host or a Git URL.

```toml
[dotfiles]
source  = "host:~/.dotfiles"
target  = "~/.dotfiles"
install = "./install.sh"
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `source` | string | *required* | `host:<path>` to copy a local directory, or a Git URL/reference to clone |
| `target` | string | `~/.dotfiles` | Destination inside the container home; must stay within that home |
| `clone_on` | string | `"host"` | Git clone location: `"host"` (reuses your SSH agent) or `"container"` (keeps the host untouched) |
| `install` | string | — | Shell command run inside the container from `target` after acquisition |

Host sources are copied in, so edits on the host arrive on the next
`dotfiles sync`. Git sources clone on the host by default, which is why
private repositories just work.

| Command | What it does |
|---------|--------------|
| `podbox dotfiles sync [name]` | Re-acquire files and rerun the install command |
| `podbox dotfiles status [name]` | Source, target, whether files are present, and whether it has run |

Failures are warnings; the container still works. If creation used
`--no-start`, the install runs on the next `dotfiles sync`.

## `[[provision]]`

An ordered list of one-shot setup commands. This is `[dotfiles] install`
generalized: as many steps as you need, each tracked independently.

```toml
[[provision]]
name = "rust-toolchain"
run  = "mise use -g rust@stable"

[[provision]]
name = "git-defaults"
run = "git config --global init.defaultBranch main"
on_failure = "abort"
env  = { FOO = "bar" }
root = false
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `name` | string | *required* | Unique per container. `^[a-z0-9][a-z0-9_-]{0,63}$`. `dotfiles` is reserved |
| `run` | string | *required* | Shell script, executed with `sh -c`. Single-line `"..."` or multi-line `"""..."""` |
| `timeout` | string | `"5m"` | Duration: `30s`, `5m`, `1h`. Must be > 0 |
| `on_failure` | string | `"warn"` | `"warn"` prints and continues; `"abort"` stops the run |
| `env` | table | `{}` | Extra variables for this step. Keys can't start with `PODBOX_` |
| `root` | bool | `false` | Run the step as root inside the container |

At most 64 steps. Order is declared order; steps run sequentially with no
dependency graph.

### How steps are stamped

When a step succeeds, podbox writes a stamp holding a hash of exactly what
produces that step's result:

- hashed: `name`, `run`, `root`, `env`
- not hashed: `timeout`, `on_failure` — they change *how* a step runs, not what
  it produces

Next time, the hash is compared. A match skips the step; a mismatch marks it
**stale** and it waits for an explicit run. It is never re-run silently.

Change `run`, add an `env` entry, flip `root`, or rename a step and it goes
stale. Tweak `timeout` and nothing happens.

### Running steps

| Command | What it does |
|---------|--------------|
| `podbox create <name>` | Runs dotfiles, then every pending/stale step |
| `podbox provision sync [name]` | Runs pending + stale steps. Exits 1 if any step failed |
| `podbox provision sync --force` | Re-runs everything, including applied steps |
| `podbox provision sync --step toolchain --step git-defaults` | Runs only the named steps |
| `podbox provision sync --dry-run` | Prints the plan, runs nothing |
| `podbox provision status [name]` | State of every step, plus orphaned stamps |
| `podbox provision status --check` | Exits 1 when anything is pending or stale — good for scripts |
| `podbox provision status --output json` | `{"steps": [{"name": ..., "state": ..., "applied_at": ...}]}` |

### Step states

| State | Meaning |
|-------|---------|
| `pending` | Declared in config, no stamp yet |
| `applied` | Stamp hash matches the current step |
| `stale` | Stamp exists but the hash differs — run `provision sync` |
| `orphaned` | Stamp on disk, no step with that name in config. Reported only |
| `unknown` | Stamps couldn't be read |

Stamps live in `~/.local/state/podbox/provision/<name>.stamp` **inside the
container home**, so their lifetime matches the home's: `podbox remove` keeps
them (a recreate won't re-run), `podbox remove --all` deletes them (a recreate
re-runs everything). `podbox clone` starts from a new home, so every step is
pending again.

### Failure handling

`on_failure = "warn"` (the default) prints the failure and continues with the
next step. The container stays usable.

`on_failure = "abort"` stops the run immediately. During `create` the command
exits 1 and the container is **kept** — not rolled back — so you can debug
before retrying.

`provision sync` exits 1 on any failure, warn included.

<details>
<summary>How a step actually executes</summary>

```
podman exec [--user root] --workdir <home> -e K=V ... <container> sh -c <run>
```

- stdin is `/dev/null`, no TTY, output streams live prefixed with the step name
- Plain non-login `sh -c`. Fish, mise shims and bash profiles are **not**
  sourced — use `bash -lc '...'` or absolute paths when you need them
- `timeout` wraps the command; on expiry the step is a failure (the
  in-container child may linger, which is why long-running work belongs in
  `[container.services]`)
- The container must be running; `provision sync` starts it if needed
- `env` values are plaintext config — never put a secret there

</details>

<details>
<summary>Environment both dotfiles install and provision steps receive</summary>

`PODBOX=1`, `PODBOX_CONTAINER`, `PODBOX_DISTRO`, `PODBOX_HOME`, and
`PODBOX_DOTFILES_DIR` when `[dotfiles]` is configured. `PODBOX_PROFILE` is set
when the configured image name matches a built-in profile. Provision steps also
get `PODBOX_STEP`. Your `env` entries are layered on top. Keys starting with
`PODBOX_` are rejected — that prefix belongs to podbox.

</details>

## Sharing setup between configs

Both are guest-only, so both are safe to inherit through `extends` and bundled
profiles. Provision steps merge by name:

- A new name **appends**
- A child step with the same `name` replaces the parent's step **in the
  parent's position**

So a shared base can declare `toolchain`, and every child can override it
without reordering the run.

## Host-side automation

Setup only ever runs *inside* the container. To run something on the **host**
around a container's lifecycle, use a standard systemd drop-in on the generated
unit. This is local policy, not part of the portable definition, on purpose.

```ini
# ~/.config/systemd/user/<name>.service.d/10-local.conf
[Service]
ExecStartPre=/home/user/bin/check-nas.sh
ExecStartPost=-/home/user/bin/notify-up.sh
ExecStopPost=-/home/user/bin/cleanup.sh
```

Why `<name>.service.d/` and not a Quadlet `.container.d/`: it targets the
generated service by name, so it keeps working across podbox's flat
(Podman 5.6–5.x) and app-subdir (6.0+) layouts, and podbox's install and
uninstall paths never have to know about it.

- A `-` prefix makes a hook non-fatal
- Hooks must be idempotent: `Restart=on-failure` re-runs them, and
  `StartLimitBurst=5` caps retries
- There is no pre-stop — systemd has no `ExecStopPre`
- `ExecStartPost` fires when the container *process* starts, not when the
  guest daemon is ready
- Run `systemctl --user daemon-reload` after editing

`podbox doctor` lists drop-ins it finds under `<name>.service.d/`. It does not
validate their contents.

## Common questions

**Editing a provision step trigger a rebuild?**
No. Steps are excluded from the build lock hash, so changing one only affects
the home.

**Can I re-run a single step?**
`podbox provision sync --step <name>`. It runs whether the step is applied,
stale, or pending.

**Can setup touch the host?**
No. Everything here runs inside the container. That's also why it's safe to
inherit through `extends` and bundled profiles.

**Can I do per-start one-shot work?**
Use a `[container.services]` entry with `restart = "never"`. A provision step
is a one-shot process, not a boot hook.

**Does `podbox provision sync` auto-start a stopped container?**
Yes, same as `podbox dotfiles sync`. `provision status` works while stopped —
it reads stamps from the host.
