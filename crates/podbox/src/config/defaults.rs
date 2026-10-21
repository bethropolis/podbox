use crate::config::enums::{GpuMode, NetworkMode};
use crate::config::types::{HostExecConfig, PackageConfig, RunConfig};

pub const EMBEDDED_DEFAULT: &str = r#"
[image]
base = "fedora:44"
name = "podbox"

[container]
    name = "podbox"
home = "~/containers/podbox"
"#;

pub fn default_true() -> bool {
    true
}

pub fn is_true(v: &bool) -> bool {
    *v
}

pub fn is_false(v: &bool) -> bool {
    !*v
}

pub fn default_shell() -> String {
    "bash".into()
}

pub fn is_default_shell(v: &str) -> bool {
    v == "bash"
}

pub fn default_pull_retry() -> u32 {
    3
}

pub fn is_default_pull_retry(v: &u32) -> bool {
    *v == 3
}

pub fn default_pull_retry_delay() -> String {
    "5s".into()
}

pub fn is_default_pull_retry_delay(v: &str) -> bool {
    v == "5s"
}

pub fn is_default_mounts(v: &super::types::MountConfig) -> bool {
    v.extra.is_empty()
}

pub fn is_default_gpu(v: &GpuMode) -> bool {
    *v == GpuMode::Auto
}

pub fn is_default_packages(v: &PackageConfig) -> bool {
    v.install.is_empty() && v.remove.is_empty() && v.manager.is_none()
}

pub fn is_default_run(v: &RunConfig) -> bool {
    v.commands.is_empty()
}

pub fn is_default_host_exec(v: &HostExecConfig) -> bool {
    !v.enabled && v.allowlist.is_none()
}

pub fn default_idle_timeout() -> String {
    "off".to_string()
}

pub fn is_default_idle_timeout(v: &str) -> bool {
    v == "off"
}

pub fn default_network_mode() -> NetworkMode {
    NetworkMode::Pasta
}

/// The Wayland interfaces the firewall refuses by screencopy/input-injection
/// rules; written TOML only names them when the list was changed by hand.
pub fn default_blocked_interfaces() -> Vec<String> {
    vec![
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
    ]
}

pub fn is_default_blocklist(list: &[String]) -> bool {
    *list == default_blocked_interfaces()
}
