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
