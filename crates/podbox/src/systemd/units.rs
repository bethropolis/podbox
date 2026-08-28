//! `systemctl` command helpers: unit start/stop/restart, enable/failed
//! queries, linger, daemon-reload, and guest-socket self-healing.
//!
//! Extracted verbatim from `systemd.rs`.

use std::process::Command;

use anyhow::{Context, Result};

/// Whether systemctl is available on this system.
pub fn is_available() -> bool {
    which::which("systemctl").is_ok()
}

/// Ensure linger is enabled for the current user.
pub fn enable_linger() -> Result<()> {
    let whoami = std::env::var("USER").unwrap_or_default();
    if whoami.is_empty() || which::which("loginctl").is_err() {
        return Ok(());
    }
    let mut cmd = Command::new("loginctl");
    cmd.args(["enable-linger", &whoami]);
    let output = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn loginctl")?
        .wait_with_output()
        .context("loginctl command failed")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Warning: enable-linger failed: {stderr}");
    } else {
        println!("Linger enabled for user.");
    }
    Ok(())
}

/// Run `systemctl --user daemon-reload`.
pub fn daemon_reload() -> Result<()> {
    if !is_available() {
        return Ok(());
    }
    let mut cmd = Command::new("systemctl");
    cmd.args(["--user", "daemon-reload"]);
    let output = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn systemctl daemon-reload")?
        .wait_with_output()
        .context("systemctl daemon-reload failed")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("daemon-reload failed: {}", stderr.trim());
    }
    Ok(())
}

/// Run `systemctl --user reset-failed` for a container's units.
pub fn reset_failed(name: &str) -> Result<()> {
    if !is_available() {
        return Ok(());
    }
    let unit_names = [
        format!("{name}.service"),
        format!("{name}.socket"),
        format!("{name}-host.service"),
        format!("{name}-proxy.service"),
        format!("{name}-compositor.service"),
    ];
    for unit in &unit_names {
        let mut cmd = Command::new("systemctl");
        cmd.args(["--user", "reset-failed", unit])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let _ = cmd.status();
    }
    Ok(())
}

/// Start and enable a socket unit.
pub fn enable_now_socket(name: &str) -> Result<()> {
    if !is_available() {
        return Ok(());
    }
    let mut cmd = Command::new("systemctl");
    cmd.args(["--user", "enable", "--now", &format!("{name}.socket")]);
    let status = cmd
        .status()
        .context("failed to spawn systemctl enable --now")?;
    if !status.success() {
        anyhow::bail!("systemctl --user enable --now {name}.socket failed");
    }
    Ok(())
}

/// Stop socket and host service units.
pub fn stop_socket_and_host(name: &str) -> Result<()> {
    if !is_available() {
        return Ok(());
    }
    for unit in [format!("{name}.socket"), format!("{name}-host.service")] {
        let mut cmd = Command::new("systemctl");
        cmd.args(["--user", "stop", &unit]);
        let _ = cmd.status();
    }
    Ok(())
}

/// Stop the Wayland compositor proxy service if it exists.
pub fn stop_compositor(name: &str) -> Result<()> {
    if !is_available() {
        return Ok(());
    }
    let mut cmd = Command::new("systemctl");
    cmd.args(["--user", "stop", &format!("{name}-compositor.service")]);
    let _ = cmd.status();
    Ok(())
}

/// Path of the guest-facing socket for a container (`%t/podbox/<name>.sock`).
pub(crate) fn guest_socket_path(name: &str) -> std::path::PathBuf {
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
        let uid = nix::unistd::getuid().as_raw();
        format!("/run/user/{uid}")
    });
    std::path::PathBuf::from(runtime)
        .join("podbox")
        .join(format!("{name}.sock"))
}

/// Restart the container's socket unit so systemd rebinds a fresh socket file.
///
/// The `.socket` unit can outlive its filesystem entry: an external unlink or
/// a RuntimeDirectory recreation leaves the unit "active (listening)" on an
/// orphaned fd while the path is gone. A container bind-mounting that path
/// then fails at create time with `statfs ...: no such file or directory`.
fn rebind_guest_socket(name: &str) -> Result<()> {
    let mut cmd = Command::new("systemctl");
    cmd.args(["--user", "restart", &format!("{name}.socket")]);
    let status = cmd.status().context("failed to spawn systemctl restart")?;
    if !status.success() {
        anyhow::bail!("systemctl --user restart {name}.socket failed");
    }
    Ok(())
}

/// Rebind the guest socket if its filesystem entry went missing.
///
/// Returns `true` when a heal was performed (socket was missing and the
/// restart succeeded).
pub(crate) fn heal_missing_guest_socket(name: &str) -> Result<bool> {
    if guest_socket_path(name).exists() {
        return Ok(false);
    }
    eprintln!(
        "Warning: {} is missing but {}.socket is active — restarting the socket unit to rebind it.",
        guest_socket_path(name).display(),
        name
    );
    rebind_guest_socket(name)?;
    Ok(true)
}

/// Start a service unit via `systemctl --user start`.
pub fn start_unit(name: &str) -> Result<()> {
    let mut cmd = Command::new("systemctl");
    cmd.args(["--user", "start", &format!("{name}.service")]);
    let status = cmd.status().context("failed to spawn systemctl start")?;
    if !status.success() {
        anyhow::bail!("systemctl start failed for '{name}.service'");
    }
    Ok(())
}

/// Stop a service unit via `systemctl --user stop`.
pub fn stop_unit(name: &str) -> Result<()> {
    let mut cmd = Command::new("systemctl");
    cmd.args(["--user", "stop", &format!("{name}.service")]);
    cmd.status()?;
    Ok(())
}

/// Restart a service unit via `systemctl --user restart`.
pub fn restart_unit(name: &str) -> Result<()> {
    let mut cmd = Command::new("systemctl");
    cmd.args(["--user", "restart", &format!("{name}.service")]);
    cmd.status()?;
    Ok(())
}

/// Check whether a unit is enabled in systemd.
pub fn is_unit_enabled(name: &str) -> bool {
    if !is_available() {
        return false;
    }
    Command::new("systemctl")
        .args([
            "--user",
            "--quiet",
            "is-enabled",
            &format!("{name}.service"),
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check whether a unit is in the failed state.
pub fn is_unit_failed(name: &str) -> bool {
    if !is_available() {
        return false;
    }
    Command::new("systemctl")
        .args(["--user", "is-failed", &format!("{name}.service")])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "failed")
        .unwrap_or(false)
}
