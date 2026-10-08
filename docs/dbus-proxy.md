---
description: Filtered D-Bus access for podbox containers via xdg-dbus-proxy — configuration, allowed services, and security model.
---

# D-Bus Proxy

Most containers either get the whole session bus or nothing. podbox sits in
the middle: a small proxy (`xdg-dbus-proxy`, run as a companion systemd unit)
forwards only the D-Bus services you allow. Notifications and link-opening
work through their portal interfaces; everything else stays unreachable
unless you add it under `[dbus]`.

## How it works

When `[dbus]` talk/own rules are configured, or `notify`/`xdg_open` need
portal access:

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

> **Warning**: adding `org.freedesktop.portal.*` (or any
> `org.freedesktop.portal.*` / `org.freedesktop.impl.portal.*` name) to
> `talk` re-grants the full portal bus surface, including host-privileged
> interfaces like `DynamicLauncher`, `Screenshot`, `ScreenCast` and
> `Settings`. Prefer the built-in interface-scoped rules described below;
> `podbox` prints a warning when it sees a portal-family `talk` entry.

## Portal access model

The `portal` preset (applied by default when `[dbus]` has no explicit rules)
does **not** add `org.freedesktop.portal.*` to the talk list. Instead, the
generated proxy exposes `org.freedesktop.portal.Desktop` through
interface-scoped rules, one per enabled capability:

| Capability | Rule granted |
|------------|--------------|
| `integration.notify` | `--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.Notification.*@/org/freedesktop/portal/desktop` |
| `integration.xdg_open` | `--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.OpenURI.*@/org/freedesktop/portal/desktop` |
| either | `--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.Request.*@/org/freedesktop/portal/desktop/request/*` (async `Request` pattern, incl. `Request.Close`) |
| either | `--broadcast=org.freedesktop.portal.Desktop=org.freedesktop.portal.Request.*@/org/freedesktop/portal/desktop/request/*` (`Request.Response` result signals) |
| either | `--call=org.freedesktop.portal.Desktop=org.freedesktop.DBus.Introspectable.*@/org/freedesktop/portal/*` (read-only introspection, needed by GIO clients to parse call arguments) |

Because `xdg-dbus-proxy` treats any granted method on a name as TALK for
that name, these rules expose exactly those portal interfaces — nothing
else. A disabled capability contributes no rules.

## Behavior matrix

| `integration.dbus` | `[dbus]` config | What the container gets |
|--------------------|-----------------|------------------------|
| `false` | any | No D-Bus access |
| `true` | default (empty), no capabilities on | Unfiltered `Volume=%t/bus:%t/bus` |
| `true` | `notify`/`xdg_open` on, or preset / talk / own set | Proxied via `xdg-dbus-proxy` with those rules plus interface-scoped portal rules for the enabled capabilities |
| `true` | `preset = ""`, empty talk + own | Unfiltered `Volume=%t/bus:%t/bus` |

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
    --call=org.freedesktop.portal.Desktop=org.freedesktop.portal.Notification.*@/org/freedesktop/portal/desktop \
    --call=org.freedesktop.portal.Desktop=org.freedesktop.portal.OpenURI.*@/org/freedesktop/portal/desktop \
    --call=org.freedesktop.portal.Desktop=org.freedesktop.portal.Request.*@/org/freedesktop/portal/desktop/request/* \
    --call=org.freedesktop.portal.Desktop=org.freedesktop.DBus.Introspectable.*@/org/freedesktop/portal/* \
    --broadcast=org.freedesktop.portal.Desktop=org.freedesktop.portal.Request.*@/org/freedesktop/portal/desktop/request/* \
    --own=org.mpris.MediaPlayer2.podbox_app
Restart=on-failure

[Install]
WantedBy=<name>.service
```

Stopping the container stops the proxy; restarting restarts it
(`PartOf=<name>.service`).

## Requirements

- `xdg-dbus-proxy` must be installed on the host system (package
  `xdg-dbus-proxy`, commonly shipped with Flatpak)
- `integration.dbus = true` (the master switch)
- A D-Bus session bus socket must be present on the host (auto-detected)

## Verification

### Test an allowed service

```bash
gdbus call --session \
    --dest org.freedesktop.Notifications \
    --object-path /org/freedesktop/Notifications \
    --method org.freedesktop.Notifications.Notify \
    "podbox" 0 "" "Hello" "Proxied message." [] {} 5000
```

Should succeed and show a host notification.

### Test isolation

```bash
gdbus call --session \
    --dest org.freedesktop.systemd1 \
    --object-path /org/freedesktop/systemd1 \
    --method org.freedesktop.DBus.Peer.Ping
```

Should fail with access denied — the proxy blocks unapproved services.

### Portal surface audit

Audited from inside a container against the proxied socket. Allowed:

| Interface | Result |
|---|---|
| `org.freedesktop.portal.Notification.AddNotification` | `()` (host notification shown) |
| `org.freedesktop.portal.OpenURI.OpenURI` | request handle returned |
| `org.freedesktop.DBus.Introspectable.Introspect` | introspection XML (needed by GIO clients) |

<details>
<summary>Denied interfaces (all return AccessDenied)</summary>

| Interface |
|---|
| `Screenshot.Screenshot` |
| `ScreenCast.CreateSession` |
| `RemoteDesktop.CreateSession` |
| `InputCapture.CreateSession` |
| `Settings.Read` |
| `Documents.Add` |
| `Account.GetUserInformation` |
| `GameMode.QueryStatus` |
| `Lockdown.GetDisabled` |
| `Print.PreparePrint` |
| `Wallpaper.SetWallpaperURI` |
| `DynamicLauncher.RequestInstallToken` |
| `FileChooser.OpenFile` |

Denial happens at the proxy before anything reaches the host — screen
capture, screenshots, file access, and launcher install stay unreachable
unless the matching capability is enabled.

</details>
