//! Companion unit generators: `.build`, `.socket`, D-Bus proxy,
//! Wayland firewall, and host socket server.
//!
//! Extracted verbatim from `quadlet.rs`; see `super` for the `.container`
//! entry point.

use std::path::Path;

use crate::config::Config;

/// Generate the `.build` Quadlet file.
pub fn generate_build(config: &Config, containerfile_path: &Path) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push("[Build]".into());
    lines.push(format!(
        "ImageTag=localhost/podbox-{}:latest",
        config.image.name
    ));
    lines.push(format!("File={}", containerfile_path.to_string_lossy()));
    lines.push(format!("Retry={}", config.image.pull_retry));
    lines.push(format!("RetryDelay={}", config.image.pull_retry_delay));

    lines.join("\n")
}

/// Generate the `.socket` Quadlet file.
pub fn generate_socket(config: &Config) -> String {
    let name = &config.container.name;
    let host_service = format!("{name}-host.service");
    let mut lines: Vec<String> = Vec::new();

    lines.push("[Unit]".into());
    lines.push(format!("Description=podbox host-guest socket -- {name}"));
    lines.push(String::new());

    lines.push("[Socket]".into());
    lines.push(format!("ListenStream=%t/podbox/{name}.sock"));
    lines.push(format!("Service={host_service}"));
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
        args.push(format!("--talk={service}"));
    }
    for rule in config.dbus_portal_calls() {
        args.push(rule);
    }
    for service in &config.dbus.own {
        args.push(format!("--own={service}"));
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
    let podbox_bin = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/usr/local/bin/podbox".into());

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
    let podbox_bin = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/usr/local/bin/podbox".into());

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
