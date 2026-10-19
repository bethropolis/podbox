use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::defaults::{
    default_pull_retry, default_pull_retry_delay, default_shell, default_true, is_default_gpu,
    is_default_host_exec, is_default_mounts, is_default_packages, is_default_pull_retry,
    is_default_pull_retry_delay, is_default_run, is_default_shell, is_false, is_true,
};
use crate::config::enums::{
    GpuMode, ImageSource, OnStop, PackageManager, ServiceRestart, XdgDirValue,
};
use crate::config::expand_tilde;

mod cache;
mod runtime;
pub use cache::{
    BuiltinCachePath, BuiltinCaches, CustomCacheConfig, HostCacheConfig, HostCachesConfig,
    SharedCachesConfig, StorageConfig, all_builtin_cache_paths, builtin_cache_mount_names,
    builtin_cache_names,
};
pub use runtime::{NetworkConfig, SecretEntry, SecretSource, SecretType, SecurityConfig};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ImageConfig {
    pub base: String,
    pub name: String,
    #[serde(rename = "image", default)]
    pub image_ref: Option<String>,
    #[serde(
        default = "default_pull_retry",
        skip_serializing_if = "is_default_pull_retry"
    )]
    pub pull_retry: u32,
    #[serde(
        default = "default_pull_retry_delay",
        skip_serializing_if = "is_default_pull_retry_delay"
    )]
    pub pull_retry_delay: String,
    #[serde(default, skip_serializing_if = "is_default_packages")]
    pub packages: PackageConfig,
    #[serde(default, skip_serializing_if = "is_default_run")]
    pub run: RunConfig,
}

impl ImageConfig {
    pub fn source(&self) -> ImageSource {
        match &self.image_ref {
            Some(ref_str) => ImageSource::Prebuilt {
                ref_str: ref_str.clone(),
            },
            None => ImageSource::Build {
                base: self.base.clone(),
            },
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct PackageConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub install: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
    /// Explicit override. `None` (omitted) means auto-detect from the image
    /// name at use time — see `resolve_prebuilt_distro` and diff's
    /// `resolve_manager`. This is why the field can stay out of the TOML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manager: Option<PackageManager>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RunConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<String>,
}

#[derive(
    Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq, strum::VariantNames,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum DotfilesCloneOn {
    #[default]
    Host,
    Container,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DotfilesConfig {
    pub source: String,
    #[serde(default = "default_dotfiles_target")]
    pub target: String,
    #[serde(default)]
    pub clone_on: DotfilesCloneOn,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<String>,
}

/// Canonical `dotfiles.target` default; shared with the schema-defaults
/// table so it cannot drift there.
pub const DOTFILES_TARGET: &str = "~/.dotfiles";

fn default_dotfiles_target() -> String {
    DOTFILES_TARGET.into()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContainerConfig {
    pub name: String,
    #[serde(deserialize_with = "deserialize_home")]
    pub home: PathBuf,
    #[serde(default = "default_shell", skip_serializing_if = "is_default_shell")]
    pub shell: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpus: Option<String>,
    /// systemd slice for this container's service cgroup.
    #[serde(
        default = "default_container_slice",
        skip_serializing_if = "is_default_container_slice"
    )]
    pub slice: String,
    /// systemd CPU scheduling weight (1..=10000).
    #[serde(
        default = "default_cpu_weight",
        skip_serializing_if = "is_default_cpu_weight"
    )]
    pub cpu_weight: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reload_cmd: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_mounts")]
    pub mounts: MountConfig,
    #[serde(default, skip_serializing_if = "ContainerEnvConfig::is_empty")]
    pub env: ContainerEnvConfig,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub services: BTreeMap<String, ServiceConfig>,
}

// Untagged so the short form serializes as a plain `name = "command"`
// string and the detailed form as a table — exactly what the custom
// reader accepts, and what the original (externally tagged) output
// could not express in TOML.
#[derive(Debug, Serialize, Clone)]
#[serde(untagged)]
pub enum ServiceConfig {
    Short(String),
    Detailed(ServiceDetailConfig),
}

// Custom deserialization so a typo inside the detailed table (for
// example `restart = "sometimes"`) surfaces the table error — which
// lists the valid values — instead of the opaque "data did not match
// any variant of untagged enum" that `#[serde(untagged)]` produces.
impl<'de> Deserialize<'de> for ServiceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = toml::Value::deserialize(deserializer)?;
        match value {
            toml::Value::String(command) => Ok(Self::Short(command)),
            toml::Value::Table(_) => value
                .try_into::<ServiceDetailConfig>()
                .map(Self::Detailed)
                .map_err(serde::de::Error::custom),
            other => Err(serde::de::Error::custom(format!(
                "expected a command string or a service table, got {}",
                other.type_str()
            ))),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServiceDetailConfig {
    pub command: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_default_restart")]
    pub restart: ServiceRestart,
}

fn is_default_restart(restart: &ServiceRestart) -> bool {
    *restart == ServiceRestart::default()
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ContainerEnvConfig {
    #[serde(flatten)]
    pub values: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forward: Vec<String>,
}

/// Resolve a `container.shell` value to the binary path terminals expect in
/// `$SHELL`. Accepts full paths verbatim; well-known names map to their
/// distro-standard locations. Anything else yields `None` (no derivation).
fn shell_binary(shell: &str) -> Option<String> {
    if shell.contains('/') {
        return Some(shell.to_string());
    }
    match shell {
        "bash" => Some("/bin/bash".into()),
        "fish" => Some("/usr/bin/fish".into()),
        "zsh" => Some("/bin/zsh".into()),
        "sh" => Some("/bin/sh".into()),
        _ => None,
    }
}

impl ContainerConfig {
    /// Effective container environment: explicit `[container.env]` values
    /// plus engine-derived defaults for well-known variables the config
    /// already implies. Explicit values always win.
    ///
    /// Current derivations (first of a general mechanism — add more here as
    /// new "config already says it" variables emerge):
    /// - `SHELL` from `container.shell`, so terminals, `enter`, and GUI
    ///   apps agree without repeating the path in every TOML.
    pub fn effective_env(&self) -> BTreeMap<String, String> {
        let mut out = self.env.values.clone();
        if !out.contains_key("SHELL") {
            if let Some(path) = shell_binary(&self.shell) {
                out.insert("SHELL".into(), path);
            }
        }
        out
    }
}

impl ContainerEnvConfig {
    fn is_empty(&self) -> bool {
        self.values.is_empty() && self.forward.is_empty()
    }
}

impl std::ops::Deref for ContainerEnvConfig {
    type Target = BTreeMap<String, String>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

fn default_container_slice() -> String {
    "podbox.slice".into()
}
fn is_default_container_slice(value: &String) -> bool {
    value == "podbox.slice"
}
fn default_cpu_weight() -> u16 {
    200
}
fn is_default_cpu_weight(value: &u16) -> bool {
    *value == 200
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct MountConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<String>,
}

fn deserialize_home<'de, D>(deserializer: D) -> std::result::Result<PathBuf, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let path = String::deserialize(deserializer)?;
    Ok(expand_tilde(&path))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HostExecEntry {
    /// Simple path string (defaults: `filter = true`, `shim = true`).
    Simple(String),
    /// Detailed configuration table.
    Detailed {
        path: String,
        #[serde(default = "default_true")]
        filter: bool,
        #[serde(default = "default_true")]
        shim: bool,
    },
}

impl HostExecEntry {
    pub fn path(&self) -> &str {
        match self {
            Self::Simple(p) => p.as_str(),
            Self::Detailed { path, .. } => path.as_str(),
        }
    }

    pub fn filter_enabled(&self) -> bool {
        match self {
            Self::Simple(_) => true,
            Self::Detailed { filter, .. } => *filter,
        }
    }

    pub fn shim_enabled(&self) -> bool {
        match self {
            Self::Simple(_) => true,
            Self::Detailed { shim, .. } => *shim,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostExecConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowlist: Option<BTreeMap<String, HostExecEntry>>,
}

impl HostExecConfig {
    pub fn resolve(&self, cmd: &str) -> Option<&HostExecEntry> {
        if !self.enabled {
            return None;
        }
        match &self.allowlist {
            Some(map) => map.get(cmd),
            None => None,
        }
    }

    /// List of alias names that should have PATH shims generated in the guest.
    pub fn guest_shims(&self) -> Vec<String> {
        if !self.enabled {
            return Vec::new();
        }
        self.allowlist
            .as_ref()
            .map(|map| {
                map.iter()
                    .filter(|(_, entry)| entry.shim_enabled())
                    .map(|(alias, _)| alias.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Schemes the host opens with `xdg-open` when the config names none.
///
/// An allowlist, not a blocklist: a blocklist misses every handler the host
/// has installed (`smb:`, `vscode:`, `steam:`, `trash:`, ...), and each one
/// is a way for guest input to reach a host app or a network share.
pub const DEFAULT_ALLOWED_URI_SCHEMES: &[&str] = &["http", "https", "mailto"];

/// Shorthand tokens a scheme name may be made of (`RFC 3986` scheme).
///
/// Only lowercase tokens are accepted: the wire comparison is
/// case-sensitive against a normalized list, so `"HTTPS"` would silently
/// refuse https traffic a user believes they enabled.
pub(crate) fn is_scheme_token(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '+' || c == '.')
}

/// `integration.xdg_open`: the bare `bool` shorthand, or a table naming the
/// schemes the host may open.
///
/// ```toml
/// [integration]
/// xdg_open = true
/// # or, relaxing the scheme allowlist:
/// xdg_open = { allowed_schemes = ["http", "https", "mailto", "smb"] }
/// ```
///
/// The table form implies `enabled = true` unless it is set explicitly:
/// a user naming schemes is asking to open something. Every extra scheme
/// reachable through `xdg-open` exposes another host handler to guest
/// input, so the list is a deliberate, documented risk
/// (`docs/guest.md` / `docs/config.md`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XdgOpenConfig {
    pub enabled: bool,
    /// Empty means "the default allowlist"; validation rejects entries that
    /// are not lowercase scheme tokens.
    pub allowed_schemes: Vec<String>,
}

impl XdgOpenConfig {
    /// The schemes a guest URI is allowed to use: the configured list, or
    /// the default when the config names none.
    pub fn scheme_list(&self) -> Vec<String> {
        if self.allowed_schemes.is_empty() {
            DEFAULT_ALLOWED_URI_SCHEMES
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        } else {
            self.allowed_schemes.clone()
        }
    }

    fn rounds_to_bool(&self) -> bool {
        self.allowed_schemes.is_empty()
            || self
                .allowed_schemes
                .iter()
                .map(String::as_str)
                .eq(DEFAULT_ALLOWED_URI_SCHEMES.iter().copied())
    }
}

impl Serialize for XdgOpenConfig {
    /// Keep the common shapes readable: a plain `xdg_open = true` while the
    /// scheme list is the default, and a table only once it is customized
    /// (or explicitly disabled with a custom list).
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        if self.rounds_to_bool() {
            return serializer.serialize_bool(self.enabled);
        }
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("enabled", &self.enabled)?;
        map.serialize_entry("allowed_schemes", &self.allowed_schemes)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for XdgOpenConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(XdgOpenVisitor)
    }
}

struct XdgOpenVisitor;

impl<'de> serde::de::Visitor<'de> for XdgOpenVisitor {
    type Value = XdgOpenConfig;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a boolean or { enabled = bool, allowed_schemes = [string, ...] }")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
        Ok(XdgOpenConfig {
            enabled: v,
            allowed_schemes: Vec::new(),
        })
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut enabled: Option<bool> = None;
        let mut schemes: Option<Vec<String>> = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "enabled" => enabled = Some(map.next_value()?),
                "allowed_schemes" => schemes = Some(map.next_value()?),
                other => {
                    return Err(serde::de::Error::unknown_field(
                        other,
                        &["enabled", "allowed_schemes"],
                    ));
                }
            }
        }
        Ok(XdgOpenConfig {
            // A table naming schemes means "open these", so the default
            // mirrors the bool shorthand's `true`.
            enabled: enabled.unwrap_or(true),
            allowed_schemes: schemes.unwrap_or_default(),
        })
    }
}

/// The `skip_serializing_if` hook for `integration.xdg_open`: write nothing
/// while the integration is off and the scheme list is untouched, so
/// default configs stay out of written TOML.
fn is_default_xdg_open(v: &XdgOpenConfig) -> bool {
    !v.enabled && v.allowed_schemes.is_empty()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IntegrationConfig {
    #[serde(default, skip_serializing_if = "is_false")]
    pub git_identity: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub wayland: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub audio: bool,
    #[serde(default, skip_serializing_if = "is_default_gpu")]
    pub gpu: GpuMode,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dbus: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub notify: bool,
    #[serde(default, skip_serializing_if = "is_default_xdg_open")]
    pub xdg_open: XdgOpenConfig,
    #[serde(default, skip_serializing_if = "is_false")]
    pub clipboard: bool,
    #[serde(default, skip_serializing_if = "is_default_host_exec")]
    pub host_exec: HostExecConfig,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ssh_agent: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub gpg_agent: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub sync_fonts: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub sync_icons: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub sync_themes: bool,
    #[serde(default, skip_serializing_if = "is_default_hardware")]
    pub hardware: HardwareConfig,
    #[serde(default, skip_serializing_if = "is_default_xdg_dirs")]
    pub xdg_dirs: XdgDirConfig,
    #[serde(default, skip_serializing_if = "is_default_export")]
    pub export: ExportConfig,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        IntegrationConfig {
            git_identity: false,
            wayland: false,
            audio: false,
            gpu: GpuMode::Auto,
            dbus: false,
            notify: false,
            xdg_open: XdgOpenConfig::default(),
            clipboard: false,
            host_exec: HostExecConfig::default(),
            ssh_agent: false,
            gpg_agent: false,
            sync_fonts: false,
            sync_icons: false,
            sync_themes: false,
            hardware: HardwareConfig::default(),
            xdg_dirs: XdgDirConfig::default(),
            export: ExportConfig::default(),
        }
    }
}

/// The `off` representations a directory may carry: the plain-false
/// shorthand or the explicit disabled table.
fn is_default_xdg_dir_value(value: &XdgDirValue) -> bool {
    match value {
        XdgDirValue::Simple(false) => true,
        XdgDirValue::Detailed {
            enabled,
            read_write,
        } => !enabled && !read_write,
        _ => false,
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct XdgDirConfig {
    #[serde(default, skip_serializing_if = "is_default_xdg_dir_value")]
    pub documents: XdgDirValue,
    #[serde(default, skip_serializing_if = "is_default_xdg_dir_value")]
    pub downloads: XdgDirValue,
    #[serde(default, skip_serializing_if = "is_default_xdg_dir_value")]
    pub pictures: XdgDirValue,
    #[serde(default, skip_serializing_if = "is_default_xdg_dir_value")]
    pub music: XdgDirValue,
    #[serde(default, skip_serializing_if = "is_default_xdg_dir_value")]
    pub videos: XdgDirValue,
    #[serde(default, skip_serializing_if = "is_default_xdg_dir_value")]
    pub desktop: XdgDirValue,
    #[serde(default, skip_serializing_if = "is_default_xdg_dir_value")]
    pub projects: XdgDirValue,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ExportConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bins: Vec<String>,
}

fn default_provision_timeout() -> String {
    "5m".into()
}

#[derive(
    Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq, strum::VariantNames,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ProvisionOnFailure {
    #[default]
    Warn,
    Abort,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
pub struct ProvisionStep {
    pub name: String,
    pub run: String,
    #[serde(default = "default_provision_timeout")]
    pub timeout: String,
    #[serde(default)]
    pub on_failure: ProvisionOnFailure,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub root: bool,
}

fn is_default_on_stop(on_stop: &OnStop) -> bool {
    *on_stop == OnStop::Keep
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LifecycleConfig {
    #[serde(default, skip_serializing_if = "is_false")]
    pub quadlet: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub autostart: bool,
    #[serde(default, skip_serializing_if = "is_default_on_stop")]
    pub on_stop: OnStop,
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_update: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_checkpoint: bool,
    #[serde(
        default = "crate::config::defaults::default_idle_timeout",
        skip_serializing_if = "crate::config::defaults::is_default_idle_timeout"
    )]
    pub idle_timeout: String,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        LifecycleConfig {
            quadlet: false,
            autostart: false,
            on_stop: OnStop::Keep,
            auto_update: false,
            auto_checkpoint: false,
            idle_timeout: crate::config::defaults::default_idle_timeout(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct SystemdConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WaylandConfig {
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub firewall: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_interfaces: Vec<String>,
}

impl Default for WaylandConfig {
    fn default() -> Self {
        Self {
            firewall: true,
            blocked_interfaces: vec![
                "zwlr_screencopy_manager_v1".into(),
                "ext_image_copy_capture_v1".into(),
                "ext_foreign_toplevel_list_v1".into(),
                "zwlr_virtual_pointer_manager_v1".into(),
                "zwlr_virtual_pointer_unstable_v1".into(),
                "zwp_virtual_keyboard_manager_v1".into(),
                "zwp_input_method_v1".into(),
                "zwp_input_method_v2".into(),
                "ext_input_method_v1".into(),
                "org_kde_kwin_fake_input".into(),
            ],
        }
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

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
pub struct HardwareConfig {
    #[serde(default, skip_serializing_if = "is_false")]
    pub joystick: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub webcam: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub yubikey: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub serial: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub kvm: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub usb: bool,
}

/// Table-level skips for sections the engine treats as absent when they
/// carry only defaults, so written TOML shows a `[table]` only once the
/// user configured something inside it.
fn is_default_hardware(hardware: &HardwareConfig) -> bool {
    *hardware == HardwareConfig::default()
}

fn is_default_xdg_dirs(xdg_dirs: &XdgDirConfig) -> bool {
    [
        &xdg_dirs.documents,
        &xdg_dirs.downloads,
        &xdg_dirs.pictures,
        &xdg_dirs.music,
        &xdg_dirs.videos,
        &xdg_dirs.desktop,
        &xdg_dirs.projects,
    ]
    .iter()
    .all(|value| is_default_xdg_dir_value(value))
}

fn is_default_export(export: &ExportConfig) -> bool {
    export.apps.is_empty() && export.bins.is_empty()
}
