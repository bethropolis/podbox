//! `--fix` actions for `podbox doctor`.
//!
//! Extracted verbatim from `doctor.rs`; see `super` for the check surface.

use std::path::Path;

use anyhow::Result;

/// Ask before applying a destructive-ish `--fix` action. Only prompts on a
/// real TTY; scripts must stay non-interactive.
pub(crate) fn confirm_fix(action: &str) -> bool {
    if !podbox::codegen::distros::is_tty() {
        return false;
    }
    dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt(format!("Fix: {action}"))
        .default(false)
        .interact()
        .unwrap_or(false)
}

/// Enable lingering for `user` so autostart units run at boot.
pub(crate) fn enable_linger(user: &str) -> Result<()> {
    let status = std::process::Command::new("loginctl")
        .args(["enable-linger", user])
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run loginctl: {e}"))?;
    if !status.success() {
        anyhow::bail!("loginctl enable-linger failed");
    }
    Ok(())
}

pub(crate) fn rewrite_memory_raw(path: &Path, suggested: &str) -> Result<()> {
    let content = std::fs::read_to_string(path)?;
    let mut value: toml::Value = content.parse()?;
    if let Some(tbl) = value.get_mut("container").and_then(|c| c.as_table_mut()) {
        tbl.insert("memory".into(), toml::Value::String(suggested.to_string()));
    } else {
        anyhow::bail!("container table not found in {}", path.display());
    }
    let out = toml::to_string_pretty(&value)?;
    std::fs::write(path, out)?;
    Ok(())
}

/// Attempt to fix a bare `memory = "2"` directly from the raw TOML file,
/// bypassing `Config::load` validation. Used when `podbox doctor --fix` is
/// invoked on an otherwise-invalid config. Returns true if a fix was applied.
pub fn try_fix_bare_memory_for_target(target_name: Option<&str>, fix: bool) -> Result<bool> {
    if !fix {
        return Ok(false);
    }
    let name = match target_name {
        Some(n) => n.to_string(),
        None => match podbox::config::read_active_context() {
            Some(n) => n,
            None => return Ok(false),
        },
    };
    let path = match podbox::config::find_config_path(&name) {
        Some(p) => p,
        None => return Ok(false),
    };
    let content = std::fs::read_to_string(&path)?;
    let value: toml::Value = match content.parse() {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    let mem = match value
        .get("container")
        .and_then(|c| c.get("memory"))
        .and_then(|m| m.as_str())
    {
        Some(m) => m.trim().to_string(),
        None => return Ok(false),
    };
    if !podbox::config::validation::is_bare_memory_digits(&mem) {
        return Ok(false);
    }
    let suggested = format!("{mem}G");
    if !confirm_fix(&format!(
        "Rewrite memory '{mem}' → '{suggested}' in {}?",
        path.display()
    )) {
        return Ok(false);
    }
    rewrite_memory_raw(&path, &suggested)?;
    println!("Fixed memory '{mem}' → '{suggested}' in {}", path.display());
    Ok(true)
}

pub(crate) fn fix_wayland_socket_ownership(socket: &Path) -> Result<()> {
    let runtime_dir = socket
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine runtime directory from socket path"))?;

    let output = std::process::Command::new("podman")
        .args(["unshare", "chown", "0:0"])
        .arg(socket)
        .arg(runtime_dir)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run podman unshare: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("podman unshare chown failed: {stderr}");
    }
    Ok(())
}
