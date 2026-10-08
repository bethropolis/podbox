---
description: Export apps and binaries from podbox containers to your host desktop — .desktop files, launcher icons, binary shims, and MIME type handling.
---

# Desktop Integration (Export)

Put container apps on your host — desktop entries with icons in your launcher,
and container tools on your `PATH` as small shell shims.

## Commands

| Command | Description |
|---------|-------------|
| `podbox export app <name>` | Export a `.desktop` application |
| `podbox export bin <name>` | Create a binary shim in `~/.local/bin` |

Applications and binaries are declared in the config under `[integration.export]`:

```toml
[integration.export]
apps = ["gedit", "nautilus"]
bins = ["rg", "gcc"]
```

## App Export

`podbox export app` extracts a desktop application from the container and makes it launchable from the host.

!!! info ""
    The container must be running to export an app — `podbox export app` uses `podman exec` to read the `.desktop` file from inside the container. Start the container first if it is stopped.

### How it works

1. Reads `/usr/share/applications/<name>.desktop` from the running container.
2. Rewrites `Exec=` to route through podbox (everything else preserved):

    ```ini
    Exec=gedit %F
    ```
    becomes:
    ```
    Exec=podbox --container "myenv" exec -- gedit %F
    ```
3. Copies the first matching icon to
   `~/.local/share/icons/podbox/<container>/<name>.<ext>`.
4. Writes `~/.local/share/applications/podbox-<container>-<name>.desktop`.
5. Runs `update-desktop-database` (a failure only warns).

<details>
<summary>Icon search paths</summary>

```
/usr/share/icons/hicolor/{48,64,128,256}x{48,64,128,256}/apps/<name>.png
/usr/share/icons/hicolor/scalable/apps/<name>.svg
```

</details>

### MIME types

`MimeType=` lines carry over unchanged — your desktop registers the
container app as a handler, and opening such a file dispatches through the
rewritten `Exec=`. No extra setup.

## Binary Export

`podbox export bin` creates a shell shim so a container binary appears on the host `PATH`.

### Generated shim

A script is written to `~/.local/bin/<name>`:

```sh
#!/bin/sh
exec podbox --container "<name>" run "<bin>" "$@"
```

The `--container` flag pins the target regardless of active context.
If `~/.local/bin` is on your `PATH` (most distros do this), the binary
just works.

## Cleanup

```bash
podbox export clean
```

Removes the container's `.desktop` files, its icon dir, and any
`~/.local/bin` shims referencing it.

!!! warning ""
    `podbox remove` doesn't unexport. Clean up before removing the container.
