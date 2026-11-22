//! `--fix` actions for `podbox doctor`.
//!
//! Extracted verbatim from `doctor.rs`; see `super` for the check surface.

use std::path::Path;

use anyhow::Result;

/// Ask before applying a destructive-ish `--fix` action. Only prompts on a
/// real TTY; scripts must stay non-interactive, so both the non-TTY refusal
/// and a declined prompt skip the fix rather than erroring. `assume_yes`
/// comes from `doctor --yes`.
pub(crate) fn confirm_fix(action: &str, assume_yes: bool) -> bool {
    let ctx = podbox::ui::Ctx::new(assume_yes, false, false);
    podbox::ui::confirm(&ctx, &format!("Fix: {action}"), false).is_ok()
}

/// Enable lingering for `user` so autostart units run at boot.
pub(crate) fn enable_linger(user: &str) -> Result<()> {
    let args = podbox::process::args(&["enable-linger", user]);
    podbox::process::run_checked("loginctl", &args)?;
    Ok(())
}

pub(crate) fn rewrite_memory_raw(path: &Path, suggested: &str) -> Result<()> {
    let content = std::fs::read_to_string(path)?;
    let mut doc: toml_edit::DocumentMut = content
        .parse()
        .map_err(|e| anyhow::anyhow!("failed to parse '{}': {e}", path.display()))?;
    let container = doc
        .get_mut("container")
        .and_then(toml_edit::Item::as_table_mut)
        .ok_or_else(|| anyhow::anyhow!("container table not found in {}", path.display()))?;
    container.insert("memory", toml_edit::value(suggested));
    podbox::fsutil::write_atomic(path, doc.to_string().as_bytes())?;
    Ok(())
}

/// Attempt to fix a bare `memory = "2"` directly from the raw TOML file,
/// bypassing `Config::load` validation. Used when `podbox doctor --fix` is
/// invoked on an otherwise-invalid config. Returns true if a fix was applied.
pub fn try_fix_bare_memory_for_target(
    target_name: Option<&str>,
    config_path: Option<&Path>,
    fix: bool,
    yes: bool,
) -> Result<bool> {
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
    let path = match config_path {
        Some(path) => path.to_path_buf(),
        None => match podbox::config::find_by_runtime_or_stem(&name) {
            Some(p) => p,
            None => return Ok(false),
        },
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
    if !confirm_fix(
        &format!(
            "Rewrite memory '{mem}' → '{suggested}' in {}?",
            path.display()
        ),
        yes,
    ) {
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

    let socket_str = socket.to_string_lossy();
    let runtime_str = runtime_dir.to_string_lossy();
    let args = podbox::process::args(&[
        "unshare",
        "chown",
        "0:0",
        socket_str.as_ref(),
        runtime_str.as_ref(),
    ]);
    let output = podbox::process::run_piped("podman", &args)
        .map_err(|e| anyhow::anyhow!("Failed to run podman unshare: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("podman unshare chown failed: {stderr}");
    }
    Ok(())
}
