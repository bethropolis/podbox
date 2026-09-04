//! Quadlet unit generation for `podbox enable`.
//!
//! Slim dispatcher module; the `.container` section emitters live in
//! [`container`], device passthrough in [`devices`], and companion units
//! in [`services`].

mod container;
mod devices;
mod services;

pub use devices::{emit_hardware_devices, emit_secrets};
pub use services::{
    generate_build, generate_compositor_service, generate_dbus_proxy_service,
    generate_host_service, generate_socket,
};

use crate::config::Config;
use crate::env::HostEnv;
use crate::xdg::ResolvedXdgDirs;

/// Generate the `.container` Quadlet file.
///
/// Pure function: all paths via HostEnv and ResolvedXdgDirs.
pub fn generate_container(config: &Config, env: &HostEnv, xdg: &ResolvedXdgDirs) -> String {
    let name = &config.container.name;
    let home_in_container = "/home/%u";
    let mut lines: Vec<String> = Vec::new();

    container::emit_unit(&mut lines, config, name);
    container::emit_container_image(&mut lines, config, name, home_in_container, env);
    container::emit_network(&mut lines, config);
    container::emit_volumes(&mut lines, config, xdg, env, name, home_in_container);
    container::emit_env(&mut lines, config, name, env);
    devices::emit_gpu(&mut lines, config, env);
    devices::emit_hardware_devices(&mut lines, config);
    devices::emit_secrets(&mut lines, config);
    container::emit_auto_update(&mut lines, config);
    container::emit_podman_args(&mut lines, config);
    container::emit_service_section(&mut lines, config);
    container::emit_install_section(&mut lines, config);

    lines.join("\n")
}
