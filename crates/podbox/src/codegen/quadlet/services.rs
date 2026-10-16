//! Companion unit generators: `.build`, `.socket`, D-Bus proxy,
//! Wayland firewall, and host socket server.
//!
//! Extracted verbatim from `quadlet.rs`; see `super` for the `.container`
//! entry point.

use std::path::Path;

use super::value;
use crate::config::Config;

/// Generate the `.build` Quadlet file.
pub fn generate_build(config: &Config, containerfile_path: &Path) -> String {
    let mut lines: Vec<String> = Vec::new();
    // Validated before codegen in every flow (CLI and Studio parse first),
    // so these pushes cannot fail in practice. This generator has no
    // diagnostics channel to report through (it must stay wasm-safe), hence
    // the local buffer.
    let mut warnings = Vec::new();

    lines.push("[Quadlet]".into());
    lines.push("DefaultDependencies=false".into());
    lines.push(String::new());

    lines.push("[Build]".into());
    value::push_raw(
        &mut lines,
        &mut warnings,
        "ImageTag",
        &config.local_image_tag(),
    );
    value::push_raw(
        &mut lines,
        &mut warnings,
        "File",
        &containerfile_path.to_string_lossy(),
    );
    value::push_raw(
        &mut lines,
        &mut warnings,
        "Retry",
        &config.image.pull_retry.to_string(),
    );
    value::push_raw(
        &mut lines,
        &mut warnings,
        "RetryDelay",
        &config.image.pull_retry_delay,
    );

    lines.join("\n")
}

/// Generate the `.socket` Quadlet file.
pub fn generate_socket(config: &Config) -> String {
    let name = &config.container.name;
    let mut lines: Vec<String> = Vec::new();
    // Same no-diagnostics-channel note as `generate_build`: inputs are
    // validated names, so these pushes cannot fail in practice.
    let mut warnings = Vec::new();

    lines.push("[Unit]".into());
    value::push_raw(
        &mut lines,
        &mut warnings,
        "Description",
        &format!("podbox host-guest socket -- {name}"),
    );
    lines.push(String::new());

    lines.push("[Socket]".into());
    value::push_composed(&mut lines, format!("ListenStream=%t/podbox/{name}.sock"));
    value::push_raw(
        &mut lines,
        &mut warnings,
        "Service",
        &format!("{name}-host.service"),
    );
    lines.push("SocketMode=0600".into());
    lines.push("DirectoryMode=0700".into());
    lines.push("RuntimeDirectory=podbox".into());
    lines.push("RuntimeDirectoryMode=0700".into());
    // Keep %t/podbox alive even when no socket unit is active. Without this,
    // systemd removes the directory when the last requesting unit stops, and
    // a later recreation can orphan sibling containers' listening sockets.
    lines.push("RuntimeDirectoryPreserve=yes".into());
    lines.push(String::new());

    lines.push("[Install]".into());
    lines.push("WantedBy=sockets.target".into());

    lines.join("\n")
}

/// Generate the companion D-Bus proxy `.service` unit.
pub fn generate_dbus_proxy_service(name: &str, config: &Config) -> Option<String> {
    if !config.use_dbus_proxy() {
        return None;
    }

    let mut args = vec![
        "unix:path=%t/bus".to_string(),
        format!("%t/podbox/{}-dbus.sock", name),
    ];

    args.push("--filter".into());

    for service in &config.dbus_effective_talk() {
        if let Ok(escaped) = value::word(service) {
            args.push(format!("--talk={escaped}"));
        }
    }
    for rule in config.dbus_portal_calls() {
        if let Ok(escaped) = value::word(&rule) {
            args.push(escaped);
        }
    }
    for service in &config.dbus.own {
        if let Ok(escaped) = value::word(service) {
            args.push(format!("--own={escaped}"));
        }
    }

    let exec_start = format!("/usr/bin/xdg-dbus-proxy {}", args.join(" "));

    Some(format!(
        r#"[Unit]
Description=D-Bus Proxy for podbox container {name}
PartOf={name}.service

[Service]
Type=simple
ExecStart={exec_start}
Restart=on-failure
RestartSec=1s

[Install]
WantedBy={name}.service
"#,
    ))
}

/// Generate the companion Wayland firewall `.service` unit.
/// Returns `None` when the Wayland proxy is disabled in config.
pub fn generate_compositor_service(name: &str, config: &Config) -> Option<String> {
    if !config.use_wayland_proxy() {
        return None;
    }
    let podbox_bin = host_binary_path();

    Some(format!(
        r#"[Unit]
Description=Wayland Firewall Proxy for podbox container {name}
PartOf={name}.service

[Service]
Type=simple
ExecStart={podbox_bin} compositor {name}
Restart=on-failure
RestartSec=1s

[Install]
WantedBy={name}.service
"#,
    ))
}

/// Generate the companion host socket server `.service` unit.
pub fn generate_host_service(name: &str) -> String {
    let podbox_bin = host_binary_path();

    format!(
        r#"[Unit]
Description=podbox host socket server -- {name}

[Service]
Type=simple
ExecStart={podbox_bin} serve {name}
Restart=on-failure
RestartSec=2s

[Install]
WantedBy={name}.socket
"#,
    )
}

/// Path of the running host binary for `ExecStart=` lines.
///
/// A hostile `current_exe` path cannot inject a second directive: control
/// characters fall back to the install default instead of being emitted.
fn host_binary_path() -> String {
    let path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/usr/local/bin/podbox".into());
    value::raw(&path).unwrap_or_else(|_| "/usr/local/bin/podbox".into())
}
