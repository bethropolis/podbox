use serde::{Deserialize, Serialize};

use crate::config::defaults::{is_default_gpu, is_default_host_exec, is_false};
use crate::config::enums::{GpuMode, XdgDirValue};

use super::host_exec::HostExecConfig;
use super::xdg_open::{XdgOpenConfig, is_default_xdg_open};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IntegrationConfig {
    #[serde(default, skip_serializing_if = "is_false")]
    pub git_identity: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub wayland: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub x11: bool,
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
            x11: false,
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

impl IntegrationConfig {
    /// True when the section is exactly what an absent `[integration]`
    /// table means; written TOML then omits the header entirely.
    pub fn is_default(&self) -> bool {
        !self.git_identity
            && !self.wayland
            && !self.x11
            && !self.audio
            && is_default_gpu(&self.gpu)
            && !self.dbus
            && !self.notify
            && is_default_xdg_open(&self.xdg_open)
            && !self.clipboard
            && is_default_host_exec(&self.host_exec)
            && !self.ssh_agent
            && !self.gpg_agent
            && !self.sync_fonts
            && !self.sync_icons
            && !self.sync_themes
            && is_default_hardware(&self.hardware)
            && is_default_xdg_dirs(&self.xdg_dirs)
            && is_default_export(&self.export)
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
