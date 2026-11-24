use super::value;
use crate::config::Config;

pub(super) fn emit_quadlet(lines: &mut Vec<String>) {
    lines.push("[Quadlet]".into());
    lines.push("DefaultDependencies=false".into());
    lines.push(String::new());
}

pub(super) fn emit_unit(
    lines: &mut Vec<String>,
    config: &Config,
    name: &str,
    warnings: &mut Vec<String>,
) {
    lines.push("[Unit]".into());
    value::push_raw(lines, warnings, "Description", &format!("podbox -- {name}"));
    value::push_raw(lines, warnings, "Requires", &format!("{name}.socket"));
    value::push_raw(lines, warnings, "After", &format!("{name}.socket"));
    for dep in &config.systemd.requires {
        value::push_raw(lines, warnings, "Requires", dep);
    }
    for dep in &config.systemd.after {
        value::push_raw(lines, warnings, "After", dep);
    }
    if config.use_dbus_proxy() {
        value::push_raw(
            lines,
            warnings,
            "Requires",
            &format!("{name}-proxy.service"),
        );
        value::push_raw(lines, warnings, "After", &format!("{name}-proxy.service"));
    }
    if config.use_wayland_proxy() {
        value::push_raw(
            lines,
            warnings,
            "Requires",
            &format!("{name}-compositor.service"),
        );
        value::push_raw(
            lines,
            warnings,
            "After",
            &format!("{name}-compositor.service"),
        );
    }
    if config.integration.x11 {
        value::push_raw(lines, warnings, "Wants", &format!("{name}-x11.service"));
        value::push_raw(lines, warnings, "After", &format!("{name}-x11.service"));
    }
    lines.push("StartLimitBurst=5".into());
    lines.push("StartLimitIntervalSec=30s".into());
    lines.push(String::new());
}

pub(super) fn emit_install_section(lines: &mut Vec<String>, config: &Config) {
    lines.push("[Install]".into());
    if config.lifecycle.autostart {
        lines.push("WantedBy=default.target".into());
    }
}
