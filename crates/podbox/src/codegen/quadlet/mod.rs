//! Quadlet unit generation for `podbox enable`.
//!
//! Slim dispatcher module; the `.container` section emitters live in
//! [`container`], cache volumes in [`caches`], device passthrough in
//! [`devices`], and companion units in [`services`].

mod caches;
mod container;
mod devices;
mod services;
pub(crate) mod value;

/// Return the Wayland socket path consumed by native clients and companion
/// services for a given host display.
pub fn wayland_socket_path(config: &Config, display: &str, name: &str) -> String {
    container::wayland_socket_path(config, display, name)
}

pub use devices::{emit_hardware_devices, emit_secrets};
pub use services::{
    generate_build, generate_compositor_service, generate_dbus_proxy_service,
    generate_host_service, generate_socket, generate_x11_service,
};

use crate::config::Config;
use crate::env::HostEnv;
use crate::xdg::ResolvedXdgDirs;

/// Generate the `.container` Quadlet file.
///
/// Pure function: all paths via HostEnv and ResolvedXdgDirs.
pub fn generate_container(config: &Config, env: &HostEnv, xdg: &ResolvedXdgDirs) -> String {
    let (output, warnings) = generate_container_with_warnings(config, env, xdg);
    for warning in &warnings {
        eprintln!("Warning: {warning}");
    }
    output
}

/// Generate the `.container` Quadlet file, collecting non-fatal diagnostics
/// (missing agent sockets, invalid env keys) instead of printing them.
/// The wasm/Studio path uses this so warnings render in the UI; `println!`
/// family panics on wasm targets, so nothing on this path may print.
pub fn generate_container_with_warnings(
    config: &Config,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
) -> (String, Vec<String>) {
    let name = &config.container.name;
    let home_in_container = "/home/%u";
    let mut lines: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    container::emit_quadlet(&mut lines);
    container::emit_unit(&mut lines, config, name, &mut warnings);
    container::emit_container_image(
        &mut lines,
        config,
        name,
        home_in_container,
        env,
        &mut warnings,
    );
    container::emit_network(&mut lines, config, &mut warnings);
    container::emit_volumes(
        &mut lines,
        config,
        xdg,
        env,
        name,
        home_in_container,
        &mut warnings,
    );
    container::emit_env(&mut lines, config, name, env, &mut warnings);
    devices::emit_gpu(&mut lines, config, env, &mut warnings);
    devices::emit_hardware_devices(&mut lines, config, env, &mut warnings);
    devices::emit_secrets(&mut lines, config, &mut warnings);
    container::emit_auto_update(&mut lines, config);
    container::emit_podman_args(&mut lines, config, &mut warnings);
    container::emit_service_section(&mut lines, config, &mut warnings);
    container::emit_install_section(&mut lines, config);

    (lines.join("\n"), warnings)
}
