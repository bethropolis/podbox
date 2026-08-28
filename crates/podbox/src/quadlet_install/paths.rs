//! Install-path helpers for Quadlet units: where `.container` files live,
//! how installed units are discovered, and the systemd user-unit dir.
//!
//! Extracted verbatim from `quadlet_install.rs`.

use std::path::PathBuf;

use crate::config;

/// Directory for user Quadlet source files.
pub(crate) fn quadlet_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| config::expand_tilde("~/.config"))
        .join("containers/systemd")
}

/// Flat install path: `~/.config/containers/systemd/<name>.container`.
pub(crate) fn flat_container_path(name: &str) -> PathBuf {
    quadlet_dir().join(format!("{name}.container"))
}

/// Application-scoped install path (Podman 6 directory/`--application` layout):
/// `~/.config/containers/systemd/<name>/<name>.container`.
pub(crate) fn application_container_path(name: &str) -> PathBuf {
    quadlet_dir().join(name).join(format!("{name}.container"))
}

/// True if a `.container` Quadlet exists in either flat or application layout.
pub fn is_installed(name: &str) -> bool {
    container_unit_path(name).is_some()
}

/// Path to the installed `.container` unit, if any (flat preferred, then app dir).
pub(crate) fn container_unit_path(name: &str) -> Option<PathBuf> {
    let flat = flat_container_path(name);
    if flat.exists() {
        return Some(flat);
    }
    let app = application_container_path(name);
    if app.exists() {
        return Some(app);
    }
    None
}

/// Names of installed `.container` units under the Quadlet dir (flat + one app level).
pub fn list_installed_names() -> Vec<String> {
    let qdir = quadlet_dir();
    let mut names = Vec::new();

    let Ok(entries) = std::fs::read_dir(&qdir) else {
        return names;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "container") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_string());
            }
            continue;
        }
        // Application subdir: <name>/<name>.container
        if path.is_dir() {
            let Some(dir_name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let nested = path.join(format!("{dir_name}.container"));
            if nested.exists() {
                names.push(dir_name.to_string());
            }
        }
    }

    names.sort();
    names.dedup();
    names
}

/// Directory for user systemd unit files.
pub(crate) fn systemd_user_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| config::expand_tilde("~/.config"))
        .join("systemd/user")
}
