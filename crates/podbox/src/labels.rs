use std::collections::HashMap;

use crate::config::{Config, GpuMode, XdgDirValue};

/// Raw OCI labels read from `podman inspect`.
pub type LabelMap = HashMap<String, String>;

/// Fetch the OCI labels for a local image tag.
pub fn fetch(image_ref: &str) -> anyhow::Result<LabelMap> {
    let output = std::process::Command::new("podman")
        .args(["inspect", "--format", "{{json .Labels}}", image_ref])
        .output()?;

    if !output.status.success() {
        return Ok(LabelMap::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let map: LabelMap = serde_json::from_str(stdout.trim()).unwrap_or_default();
    Ok(map)
}

/// Apply podbox label defaults to a Config, with the user config winning
/// on every field that was explicitly set.
///
/// Accepts both `podbox.*` (preferred) and `podmgr.*` (compat fallback)
/// label keys. New keys take precedence when both are present.
pub fn apply_defaults(config: &mut Config, labels: &LabelMap) {
    // Schema check: accept both new and old key
    match labels
        .get("podbox.schema")
        .or_else(|| labels.get("podmgr.schema"))
        .map(|s| s.as_str())
    {
        Some("1") => {}
        Some(v) => {
            eprintln!(
                "Warning: image declares podbox.schema={}, host supports 1. \
                 Ignoring image labels.",
                v
            );
            return;
        }
        None => return,
    }

    let int = &mut config.integration;

    apply_bool_compat(
        labels,
        "podbox.integration.wayland",
        "podmgr.integration.wayland",
        &mut int.wayland,
    );
    apply_bool_compat(
        labels,
        "podbox.integration.audio",
        "podmgr.integration.audio",
        &mut int.audio,
    );
    apply_bool_compat(
        labels,
        "podbox.integration.dbus",
        "podmgr.integration.dbus",
        &mut int.dbus,
    );
    apply_bool_compat(
        labels,
        "podbox.integration.notify",
        "podmgr.integration.notify",
        &mut int.notify,
    );
    apply_bool_compat(
        labels,
        "podbox.integration.xdg_open",
        "podmgr.integration.xdg_open",
        &mut int.xdg_open,
    );
    apply_bool_compat(
        labels,
        "podbox.integration.clipboard",
        "podmgr.integration.clipboard",
        &mut int.clipboard,
    );
    apply_bool_compat(
        labels,
        "podbox.integration.sync_fonts",
        "podmgr.integration.sync_fonts",
        &mut int.sync_fonts,
    );
    apply_bool_compat(
        labels,
        "podbox.integration.sync_icons",
        "podmgr.integration.sync_icons",
        &mut int.sync_icons,
    );
    apply_bool_compat(
        labels,
        "podbox.integration.sync_themes",
        "podmgr.integration.sync_themes",
        &mut int.sync_themes,
    );

    apply_xdg_compat(
        labels,
        "podbox.xdg_dirs.documents",
        "podmgr.xdg_dirs.documents",
        &mut int.xdg_dirs.documents,
    );
    apply_xdg_compat(
        labels,
        "podbox.xdg_dirs.downloads",
        "podmgr.xdg_dirs.downloads",
        &mut int.xdg_dirs.downloads,
    );
    apply_xdg_compat(
        labels,
        "podbox.xdg_dirs.pictures",
        "podmgr.xdg_dirs.pictures",
        &mut int.xdg_dirs.pictures,
    );
    apply_xdg_compat(
        labels,
        "podbox.xdg_dirs.music",
        "podmgr.xdg_dirs.music",
        &mut int.xdg_dirs.music,
    );
    apply_xdg_compat(
        labels,
        "podbox.xdg_dirs.videos",
        "podmgr.xdg_dirs.videos",
        &mut int.xdg_dirs.videos,
    );
    apply_xdg_compat(
        labels,
        "podbox.xdg_dirs.desktop",
        "podmgr.xdg_dirs.desktop",
        &mut int.xdg_dirs.desktop,
    );
    apply_xdg_compat(
        labels,
        "podbox.xdg_dirs.projects",
        "podmgr.xdg_dirs.projects",
        &mut int.xdg_dirs.projects,
    );

    // GPU: accept both keys
    let gpu_key = if labels.contains_key("podbox.integration.gpu") {
        "podbox.integration.gpu"
    } else {
        "podmgr.integration.gpu"
    };
    if let Some(gpu_str) = labels.get(gpu_key) {
        if config.integration.gpu == GpuMode::Auto {
            config.integration.gpu = match gpu_str.as_str() {
                "true" => GpuMode::Enabled,
                "false" => GpuMode::Disabled,
                "nvidia" => GpuMode::Nvidia,
                _ => GpuMode::Auto,
            };
        }
    }

    // Shell: accept both keys
    let shell_key = if labels.contains_key("podbox.default_shell") {
        "podbox.default_shell"
    } else {
        "podmgr.default_shell"
    };
    if let Some(shell) = labels.get(shell_key) {
        if config.container.shell == "fish" {
            config.container.shell = shell.clone();
        }
    }
}

fn apply_bool(labels: &LabelMap, key: &str, field: &mut bool) {
    if let Some(v) = labels.get(key) {
        *field = v == "true";
    }
}

fn apply_bool_compat(labels: &LabelMap, new_key: &str, old_key: &str, field: &mut bool) {
    // New key takes precedence; fall back to old key
    if labels.contains_key(new_key) {
        apply_bool(labels, new_key, field);
    } else {
        apply_bool(labels, old_key, field);
    }
}

fn apply_xdg_compat(labels: &LabelMap, new_key: &str, old_key: &str, field: &mut XdgDirValue) {
    let key = if labels.contains_key(new_key) {
        new_key
    } else {
        old_key
    };
    if let Some(v) = labels.get(key) {
        if v == "true" {
            *field = XdgDirValue::Simple(true);
        }
    }
}
