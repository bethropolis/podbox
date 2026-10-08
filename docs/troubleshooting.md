---
description: Fix common podbox issues — find your symptom, run the fix.
---

# Troubleshooting

```bash
podbox doctor         # diagnoses most issues, explains the fix
podbox doctor --fix   # offers to fix them
```

| Symptom | Fix |
|---------|-----|
| Container won't start | [Container won't start](#container-wont-start) |
| Hangs on startup | [D-Bus proxy](#d-bus-proxy-fails-or-container-hangs-on-startup) |
| GUI apps don't appear | [Wayland socket](#gui-apps-dont-appear-wayland-socket-errors) |
| `notify-send`, `xdg-open`, clipboard dead | [Interceptors](#interceptors-not-working) |
| Permission errors in mounted dirs | [UID mismatch](#uid-mismatch-or-permission-errors) |
| SSH agent not forwarded | [SSH agent](#ssh-agent-not-forwarding) |
| Stale image or failed build | [Build](#build-fails-or-produces-a-stale-image) |
| `podbox shell` hangs | [Shell missing](#container-starts-but-podbox-shell-hangs) |
| Commands hit the wrong container | [Targeting](#commands-target-the-wrong-container) |

## Quick recovery

Won't start? Run this first:

```bash
podbox recover [NAME]        # guided fix; --yes skips prompts
```

Safe and idempotent: reloads systemd, reinstalls Quadlets, rebuilds the
image only if missing, then restarts. Never touches your home or config —
only `podbox remove --all` deletes those.

<details>
<summary>What doctor and recover actually do</summary>

`podbox doctor` groups checks into **Host / Container / Integration** and
ends with a plain-language **Host exposure** summary (network mode, D-Bus
rules, clipboard, agents, host-exec allowlist, extra mounts). Exits non-zero
when anything fails, so scripts can gate on it.

`podbox recover` walks four steps — daemon-reload + reset-failed, Quadlet
reinstall, image rebuild (only when missing), stop/start — confirming each
on a TTY.

</details>

## Container won't start

```bash
podman ps -a --filter name=<name>      # check container state
podbox logs                            # container output
podbox enable --dry-run                # inspect Quadlets without writing
podbox enable                          # safe to re-run (uses --replace)
```

If Quadlets are installed, also:

```bash
systemctl --user status <name>.service
```

## D-Bus proxy fails or container hangs on startup

`xdg-dbus-proxy` is missing. Install it, or turn D-Bus off:

```bash
which xdg-dbus-proxy   # should print a path
```

```toml
[integration]
dbus = false           # if you don't need D-Bus
```

## GUI apps don't appear / Wayland socket errors

The socket path is baked in at `podbox enable` time. If it changed (e.g.
after a reboot), regenerate:

```bash
echo $WAYLAND_DISPLAY                    # should print wayland-0 or similar
podbox enable                            # regenerate Quadlets (idempotent)
podbox stop && podbox start
```

## Interceptors not working

`notify-send`, `xdg-open`, clipboard, and host-exec all go through the
guest daemon. If it can't reach the host socket, they're silently skipped.

```bash
podbox exec -- ps aux | grep podbox-guest       # daemon running?
podbox exec -- echo $PATH                       # should include /run/podbox/bin
podbox exec -- cat /etc/environment.d/podbox.conf   # PATH injection file
```

## UID mismatch or permission errors

Host UID 1000 maps to container UID 999 (`UserNS=keep-id`, shifted by 1).

- Never `chown` a bind-mounted dir from inside the container — it changes
  ownership on the host too.
- Files owned by `nobody`? The mount predates the UID mapping. Stop the
  container, fix ownership on the host, start again.

## SSH agent not forwarding

Needs Podman ≥ 5.6 and:

```toml
[integration]
ssh_agent = true
```

```bash
podbox doctor                             # checks Podman version
grep ssh_agent ~/.config/podbox/<name>.toml
```

On Podman 5.5 the socket path is baked at `enable` time — if `$SSH_AUTH_SOCK`
changed since (e.g. new login), re-run `podbox disable && podbox enable`.

## Build fails or produces a stale image

```bash
podbox build --rebuild
```

Still broken? Clear the build context and rebuild:

```bash
rm -rf ~/.local/share/podbox/<name>/
podbox build --rebuild
```

Custom-build Containerfiles regenerate from TOML on every build — package
and `run.commands` changes are picked up automatically.

## Container starts but `podbox shell` hangs

The shell in `container.shell` isn't installed in the image. Add it to
`[image.packages].install`, then `podbox build --rebuild`.

## Commands target the wrong container

Resolution order: positional `[NAME]` → `-C` → `$PODBOX_CONTAINER` →
active context → picker → single config → embedded default.

```bash
podbox use                  # show current context
podbox use <name>           # set it
podbox use --clear          # clear it
```
