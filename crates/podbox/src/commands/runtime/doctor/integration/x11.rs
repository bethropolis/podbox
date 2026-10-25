//! X11 bridge dependency and runtime checks.

use std::process::Command;

use podbox::config::Config;
use podbox::env::HostEnv;

use super::super::Check;

pub(crate) fn check_x11(config: &Config, env: &HostEnv) -> Vec<Check> {
    if !config.integration.x11 {
        return Vec::new();
    }

    let mut checks = Vec::new();
    match which::which("xwayland-satellite") {
        Ok(path) => checks.push(Check::new(
            "xwayland-satellite",
            "pass",
            format!("found at {}", path.display()),
        )),
        Err(_) => checks.push(Check::new(
            "xwayland-satellite",
            "fail",
            "not found; install xwayland-satellite on the host",
        )),
    }
    match which::which("Xwayland") {
        Ok(path) => checks.push(Check::new(
            "Xwayland",
            "pass",
            format!("found at {}", path.display()),
        )),
        Err(_) => checks.push(Check::new(
            "Xwayland",
            "fail",
            "not found; install the Xwayland package on the host",
        )),
    }

    let unit = format!("{}-x11.service", config.container.name);
    let active = Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", &unit])
        .status()
        .is_ok_and(|status| status.success());
    checks.push(Check::new(
        "X11 bridge service",
        if active { "pass" } else { "warn" },
        if active {
            "active"
        } else {
            "inactive; start the container and check systemctl --user status"
        },
    ));

    let socket = env
        .xdg_runtime_dir
        .join("podbox")
        .join(format!("{}-x11", config.container.name))
        .join("X0");
    checks.push(Check::new(
        "X11 socket",
        if socket.exists() { "pass" } else { "warn" },
        if socket.exists() {
            socket.display().to_string()
        } else {
            format!("{} does not exist", socket.display())
        },
    ));

    checks
}
