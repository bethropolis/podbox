# D-Bus Proxy

By default, `integration.dbus = true` bind-mounts the host's D-Bus session
bus socket (`%t/bus`) directly into the container — giving the container
**unfiltered access** to the entire host session bus.

For better isolation, `podbox` can generate a companion systemd unit that
runs `xdg-dbus-proxy` to filter which D-Bus services the container can
interact with.

---

## How it works

When `[dbus]` talk or own rules are configured:

1. `podbox enable` writes an additional file:
   ```
   ~/.config/containers/systemd/<name>-proxy.service
   ```

2. The generated `.container` quadlet gains:
   ```
   Requires=<name>-proxy.service
   After=<name>-proxy.service
   ```

3. Instead of `Volume=%t/bus:%t/bus`, the container gets the proxy socket:
   ```
   Volume=%t/podbox/<name>-dbus.sock:/run/podbox/dbus.sock:ro
   Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/podbox/dbus.sock
   ```

4. The proxy service runs `xdg-dbus-proxy`, which forwards only the
   explicitly allowed D-Bus services to the container.

---

## Configuration

```toml
[dbus]
talk = [
    "org.freedesktop.Notifications",
    "org.mpris.MediaPlayer2.*",
]
own = [
    "org.mpris.MediaPlayer2.podbox_app",
]
```

| Key | Type | Description |
|-----|------|-------------|
| `talk` | string[] | D-Bus services the container can call (two-way communication) |
| `own` | string[] | D-Bus services the container can register on the host bus |

Wildcards (`*`) are supported per the `xdg-dbus-proxy` filtering rules.

---

## Behavior matrix

| `integration.dbus` | `[dbus]` talk/own | What the container gets |
|--------------------|-------------------|------------------------|
| `false` | any | No D-Bus access at all |
| `true` | empty (default) | Unfiltered `Volume=%t/bus:%t/bus` |
| `true` | at least one rule | Proxy socket via `xdg-dbus-proxy` |

---

## Generated proxy unit

When rules are present, a companion systemd service is generated at
`~/.config/containers/systemd/<name>-proxy.service`:

```ini
[Unit]
Description=D-Bus Proxy for podbox container <name>
PartOf=<name>.service

[Service]
Type=simple
RuntimeDirectory=podbox
ExecStart=/usr/bin/xdg-dbus-proxy \
    unix:path=%t/bus \
    %t/podbox/<name>-dbus.sock \
    --talk=org.freedesktop.Notifications \
    --talk=org.mpris.MediaPlayer2.* \
    --own=org.mpris.MediaPlayer2.podbox_app
Restart=on-failure

[Install]
WantedBy=<name>.service
```

The proxy's lifecycle is tied to the container via `PartOf=<name>.service`.
Stopping the container stops the proxy; restarting the container restarts
the proxy.

---

## Requirements

- `xdg-dbus-proxy` must be installed on the host system (package
  `xdg-dbus-proxy`, commonly shipped with Flatpak)
- `integration.dbus = true` (the master switch)
- A D-Bus session bus socket must be present on the host (auto-detected)

---

## Verification

### Test an allowed service

```bash
gdbus call --session \
    --dest org.freedesktop.Notifications \
    --object-path /org/freedesktop/Notifications \
    --method org.freedesktop.Notifications.Notify \
    "podbox" 0 "" "Hello" "Proxied message." [] {} 5000
```

This should succeed and show a desktop notification on the host.

### Test isolation

```bash
gdbus call --session \
    --dest org.freedesktop.systemd1 \
    --object-path /org/freedesktop/systemd1 \
    --method org.freedesktop.DBus.Peer.Ping
```

This should fail with an access denied error — the proxy blocks the
unapproved `org.freedesktop.systemd1` service.
