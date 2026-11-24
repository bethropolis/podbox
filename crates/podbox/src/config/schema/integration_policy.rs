use crate::config::{Config, DbusPreset};

impl Config {
    /// Effective D-Bus talk list.
    ///
    /// The `portal` preset no longer contributes `org.freedesktop.portal.*`
    /// here — the portal name is exposed through interface-scoped
    /// `--call=`/`--broadcast=` rules instead (see [`Self::dbus_portal_calls`]),
    /// so host-privileged portal interfaces (DynamicLauncher, Screenshot,
    /// ScreenCast, Settings, ...) are unreachable from the container.
    pub fn dbus_effective_talk(&self) -> Vec<String> {
        self.dbus.effective_talk()
    }

    /// Interface-scoped `--call=`/`--broadcast=` rules for the XDG portal name.
    ///
    /// The portal preset no longer grants `org.freedesktop.portal.*` wholesale
    /// via `--talk=`. Instead, only the interfaces actually needed by the
    /// enabled capabilities are exposed as `xdg-dbus-proxy` rules scoped to
    /// `org.freedesktop.portal.Desktop`:
    ///
    /// - `integration.notify` → `org.freedesktop.portal.Notification.*`
    /// - `integration.xdg_open` → `org.freedesktop.portal.OpenURI.*`
    ///
    /// Portals use the async Request pattern, so the `Request` interface on the
    /// `/org/freedesktop/portal/desktop/request/*` subtree is always allowed
    /// alongside (method calls for `Request.Close`, and the `Request.Response`
    /// broadcast signal that carries the actual result). A read-only
    /// `org.freedesktop.DBus.Introspectable` rule is added so GIO-based clients
    /// can introspect the service (gdbus needs the XML to parse arguments).
    pub fn dbus_portal_calls(&self) -> Vec<String> {
        let mut rules: Vec<String> = Vec::new();
        if self.integration.notify {
            rules.push(
                "--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.Notification.*@/org/freedesktop/portal/desktop"
                    .into(),
            );
        }
        if self.integration.xdg_open.enabled {
            rules.push(
                "--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.OpenURI.*@/org/freedesktop/portal/desktop"
                    .into(),
            );
        }
        if self.integration.notify || self.integration.xdg_open.enabled {
            // Portals use the async Request pattern, so the `Request` interface
            // on the `/org/freedesktop/portal/desktop/request/*` subtree is
            // always allowed alongside (method calls for `Request.Close`, and
            // the `Request.Response` signal that carries the actual result).
            rules.push(
                "--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.Request.*@/org/freedesktop/portal/desktop/request/*"
                    .into(),
            );
            rules.push(
                "--broadcast=org.freedesktop.portal.Desktop=org.freedesktop.portal.Request.*@/org/freedesktop/portal/desktop/request/*"
                    .into(),
            );
            // GIO-based clients introspect the service before calling (gdbus
            // uses the resulting XML to parse arguments). Introspection is
            // read-only (returns interface metadata) and doesn't grant any
            // method access, so it is allowed over the portal subtree.
            rules.push(
                "--call=org.freedesktop.portal.Desktop=org.freedesktop.DBus.Introspectable.*@/org/freedesktop/portal/*"
                    .into(),
            );
            // Nearly every GTK app reads theme preferences via Settings.Read
            // at startup; without this it falls back with a warning. Read-only
            // like introspection — no portal privilege beyond preferences.
            rules.push(
                "--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.Settings.*@/org/freedesktop/portal/desktop"
                    .into(),
            );
        }
        rules
    }

    pub fn use_dbus_proxy(&self) -> bool {
        self.integration.dbus
            && (self.dbus.preset != DbusPreset::None
                || !self.dbus_effective_talk().is_empty()
                || !self.dbus_portal_calls().is_empty()
                || !self.dbus.own.is_empty())
    }

    pub fn use_wayland_proxy(&self) -> bool {
        self.integration.wayland && self.wayland.firewall
    }
}
