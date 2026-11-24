use serde::{Deserialize, Serialize};

use crate::config::defaults::{default_true, is_true};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WaylandConfig {
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub firewall: bool,
    /// Host Wayland interfaces the firewall refuses. In the absence of any
    /// explicitly set list, these are screencopy/input-injection protocols a
    /// container must not get for free.
    #[serde(
        default = "crate::config::defaults::default_blocked_interfaces",
        skip_serializing_if = "crate::config::defaults::is_default_blocklist"
    )]
    pub blocked_interfaces: Vec<String>,
}

impl Default for WaylandConfig {
    fn default() -> Self {
        Self {
            firewall: true,
            blocked_interfaces: crate::config::defaults::default_blocked_interfaces(),
        }
    }
}

impl WaylandConfig {
    /// True when this config is exactly what an absent `[wayland]` table
    /// means; the whole table can then stay out of written TOML.
    pub fn is_default(&self) -> bool {
        self.firewall
            && self.blocked_interfaces == crate::config::defaults::default_blocked_interfaces()
    }
}

#[derive(
    Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq, strum::VariantNames,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum DbusPreset {
    Flatpak,
    Gnome,
    Kde,
    Portal,
    /// No preset contribution. Explicit opt-out; anything else implies the proxy.
    #[default]
    None,
}

fn is_default_dbus_preset(preset: &DbusPreset) -> bool {
    *preset == DbusPreset::None
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct DbusConfig {
    #[serde(default, skip_serializing_if = "is_default_dbus_preset")]
    pub preset: DbusPreset,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub talk: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub own: Vec<String>,
}

impl DbusConfig {
    /// True when the section is exactly what an absent `[dbus]` table
    /// means; written TOML then omits the header entirely.
    pub fn is_default(&self) -> bool {
        is_default_dbus_preset(&self.preset) && self.talk.is_empty() && self.own.is_empty()
    }

    pub fn effective_talk(&self) -> Vec<String> {
        let mut result = self.talk.clone();
        for svc in dbus_preset_talk(self.preset) {
            if !result.contains(&svc.to_string()) {
                result.push(svc.to_string());
            }
        }
        result
    }
}

/// Talk entries contributed by a D-Bus preset.
///
/// Portal names are deliberately absent: `org.freedesktop.portal.*` is never
/// granted wholesale (a dedicated validation warning fires if it appears in
/// `talk`). Portal access comes through the interface-scoped `--call=` rules
/// derived from the enabled capabilities, plus `Notifications` talk here so
/// that an explicitly set preset always implies the proxy.
pub fn dbus_preset_talk(preset: DbusPreset) -> &'static [&'static str] {
    match preset {
        DbusPreset::Flatpak => &["org.freedesktop.Flatpak", "org.freedesktop.Flatpak.*"],
        DbusPreset::Gnome => &[
            "org.gnome.Shell",
            "org.gnome.Shell.*",
            "org.gnome.ScreenSaver",
            "org.gnome.Mutter.*",
            "org.gnome.keyring.*",
        ],
        DbusPreset::Kde => &["org.kde.*"],
        DbusPreset::Portal => &["org.freedesktop.Notifications"],
        DbusPreset::None => &[],
    }
}
