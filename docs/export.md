# Desktop Integration (Export)

`podmgr` can expose applications and binaries from inside the container to
the host desktop — generating `.desktop` files, extracting icons, and
creating shell shims.

---

## Commands

| Command | Description |
|---------|-------------|
| `podmgr export app <name>` | Export a `.desktop` application |
| `podmgr export bin <name>` | Create a binary shim in `~/.local/bin` |

Applications and binaries are declared in the config under
`[integration.export]`:

```toml
[integration.export]
apps = ["gedit", "nautilus"]
bins = ["rg", "gcc"]
```

---

## App Export (`podmgr export app`)

### What it does

1. **Reads the `.desktop` file** from the container at
   `/usr/share/applications/<name>.desktop` via `podman exec`.

2. **Rewrites the `Exec=` line** so that launching the desktop entry runs
   through `podmgr exec` inside the container:

   ```
   Exec=gedit %F
   ```
   becomes:
   ```
   Exec=podmgr --container myenv exec -- gedit %F
   ```

   All other keys (`Name=`, `Icon=`, `MimeType=`, etc.) are preserved
   unchanged.

3. **Extracts the icon** by trying common paths inside the container:
   ```
   /usr/share/icons/hicolor/{48,64,128,256}x{48,64,128,256}/apps/<name>.png
   /usr/share/icons/hicolor/scalable/apps/<name>.svg
   ```
   The first match is copied to:
   ```
   ~/.local/share/icons/podmgr/<container>/<name>.<ext>
   ```

4. **Writes the `.desktop` file** to:
   ```
   ~/.local/share/applications/podmgr-<container>-<name>.desktop
   ```

5. **Runs `update-desktop-database`** on the applications directory
   (failure is non-fatal; a warning is printed).

### MIME type handling

`MimeType=` lines in the original `.desktop` file are preserved as-is.
The host desktop environment registers the container app as a handler
for those MIME types. When a user opens a file of that type, the
rewritten `Exec=` line dispatches through `podmgr exec`.

---

## Binary Export (`podmgr export bin`)

Creates a shell shim in `~/.local/bin/<name>`:

```sh
#!/bin/sh
exec podmgr --container "<name>" exec -- "<bin>" "$@"
```

The shim is executable (`chmod 755`). If `~/.local/bin` is on the user's
`PATH` (which most distributions add by default), the binary appears as
if installed locally.

---

## Cleanup

Remove exported files for a container by calling:

```rust
podmgr::export::unexport_all(container_name)
```

This removes:
- All `~/.local/share/applications/podmgr-<container>-*.desktop` files
- `~/.local/share/icons/podmgr/<container>/` directory tree
- Any shims in `~/.local/bin/` whose content references the container name

Note: `podmgr remove` does **not** automatically call unexport. Run
`podmgr export` commands or call `unexport_all` separately before
removing the container.
