//! Pre-creation of bind-mount targets inside the container home.
//!
//! Podman creates a missing bind-mount target inside the container as root,
//! which `keep-id` maps to host uid 100000 — so any target podbox mounts into
//! `$HOME` but never creates on the host comes back root-owned and unwritable
//! for the container user (mise state, cargo registries, theme dirs, ...).
//!
//! The destinations here must stay in step with the `Volume=` lines emitted by
//! [`crate::codegen::quadlet`]; a test parses the generated unit and asserts
//! every home-relative destination is covered.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::Config;
use crate::env::HostEnv;
use crate::xdg::ResolvedXdgDirs;

/// Container home as rendered in generated units.
const UNIT_HOME: &str = "/home/%u";

/// Create every bind-mount target under the container home, with the host
/// user's ownership, so podman never has to create one as root. Idempotent:
/// anything already present is left untouched.
pub fn precreate_bind_targets(config: &Config, env: &HostEnv, xdg: &ResolvedXdgDirs) -> Result<()> {
    for target in bind_targets_under_home(config, env, xdg) {
        std::fs::create_dir_all(&target)
            .with_context(|| format!("failed to create mount target '{}'", target.display()))?;
    }
    Ok(())
}

/// Host paths to create, all under `config.container.home`. Pure: split out
/// for testing.
pub fn bind_targets_under_home(
    config: &Config,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
) -> Vec<PathBuf> {
    let home = &config.container.home;
    let mut targets: Vec<PathBuf> = Vec::new();

    // A destination whose source is an existing file only needs its parent;
    // everything else (dirs, missing sources) is created wholesale, matching
    // what podman would create — except with the host user's ownership.
    let mut push = |rel: &str, source: Option<&Path>| {
        if rel.is_empty() {
            return;
        }
        let rel = match source {
            Some(src) if src.is_file() => match Path::new(rel).parent() {
                Some(parent) if !parent.as_os_str().is_empty() => {
                    parent.to_string_lossy().into_owned()
                }
                _ => return,
            },
            _ => rel.to_string(),
        };
        let path = home.join(&rel);
        if !targets.contains(&path) {
            targets.push(path);
        }
    };

    // Shared caches are named volumes, always directories.
    for path in config.storage.shared_caches.enabled_paths() {
        if let Some(rel) = under_home(path.container_path, &env.username) {
            push(&rel, None);
        }
    }
    for cache in &config.storage.shared_caches.custom {
        if let Some(rel) = under_home(&cache.container_path, &env.username) {
            push(&rel, None);
        }
    }
    // Host caches are bind mounts; built-ins are always directories.
    for path in config.storage.host_caches.enabled_paths() {
        if let Some(rel) = under_home(path.container_path, &env.username) {
            push(&rel, None);
        }
    }
    for cache in &config.storage.host_caches.custom {
        if let Some(rel) = under_home(&cache.container_path, &env.username) {
            let source = expand_host_source(&cache.host_path);
            let source_opt = source.exists().then_some(source.as_path());
            push(&rel, source_opt);
        }
    }

    // Selective XDG dirs land directly under the container home.
    for (name, dir) in [
        ("Documents", &xdg.documents),
        ("Downloads", &xdg.downloads),
        ("Pictures", &xdg.pictures),
        ("Music", &xdg.music),
        ("Videos", &xdg.videos),
        ("Desktop", &xdg.desktop),
        ("Projects", &xdg.projects),
    ] {
        if dir.is_some() {
            push(name, None);
        }
    }

    // Visual integration, mirroring the conditions in
    // `codegen::quadlet::container::emit_volumes`.
    let host_home = dirs_home();
    if config.integration.sync_themes {
        if host_home.join(".themes").exists() {
            push(".themes", None);
        }
        if env.host_has_local_share_themes {
            push(".local/share/themes", None);
        }
    }
    if config.integration.sync_icons {
        if host_home.join(".icons").exists() {
            push(".icons", None);
        }
        if env.host_has_local_share_icons {
            push(".local/share/icons", None);
        }
    }
    if config.integration.sync_fonts {
        if host_home.join(".fonts").exists() {
            push(".fonts", None);
        }
        if env.host_has_local_share_fonts {
            push(".local/share/fonts", None);
        }
    }

    // Extra mounts: destination is the middle field. Only destinations under
    // the container home can be pre-created; anything else lives in the
    // ephemeral container rootfs, where root ownership does not persist.
    for mount in &config.container.mounts.extra {
        let mut parts = mount.splitn(3, ':');
        let source = parts.next().unwrap_or("");
        let Some(dest) = parts.next() else {
            continue;
        };
        if let Some(rel) = under_home(dest, &env.username) {
            let expanded = expand_host_source(source);
            let source_opt = expanded.exists().then_some(expanded.as_path());
            push(&rel, source_opt);
        }
    }

    targets
}

/// Strip a container-home prefix, returning the home-relative remainder.
/// Accepts the `~/` shorthand, the literal unit form `/home/%u`, and the
/// resolved `/home/<user>`. Anything else (system paths, runtime sockets)
/// is outside the home bind and returns `None`.
fn under_home(dest: &str, username: &str) -> Option<String> {
    if let Some(rest) = dest.strip_prefix("~/") {
        return Some(rest.to_string());
    }
    if dest == "~" {
        return Some(String::new());
    }
    let resolved = format!("/home/{username}");
    for prefix in [UNIT_HOME, resolved.as_str()] {
        if let Some(rest) = dest.strip_prefix(&format!("{prefix}/")) {
            return Some(rest.to_string());
        }
        if dest == prefix {
            return Some(String::new());
        }
    }
    None
}

/// Expand `~/` and `%h/` against the real host home for source-type checks.
fn expand_host_source(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs_home().join(rest);
    }
    if path == "~" {
        return dirs_home();
    }
    if let Some(rest) = path.strip_prefix("%h/") {
        return dirs_home().join(rest);
    }
    PathBuf::from(path)
}

fn dirs_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"))
}
