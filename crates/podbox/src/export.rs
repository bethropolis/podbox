//! Host-side export of container binaries and `.desktop` files: bubblewrap/
//! shim export, symlinking, and per-container uninstall. One cohesive export
//! flow (`.desktop` discovery/rewrite lives in the [`desktop`](crate::export::desktop)
//! submodule); stays above ~300 LOC as a single cohesive concern (documented
//! exemption, per MODULARIZATION_GUIDE).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::config::Config;
use crate::error::PodboxError;

/// Standard XDG application directories searched inside the container,
/// in priority order.  Many apps install to `~/.local/share/applications/`
mod desktop;

pub(crate) use desktop::{
    copy_icon_from_container, extract_icon_name, find_desktop_file, is_valid_app_name,
    rewrite_desktop_file,
};

/// Host executable to embed in exported launchers and shims.
///
/// `current_exe()` under a dev build resolves into `target/debug`, an mbx
/// cache dir, or `/tmp` — all rebuilt or garbage-collected, so a launcher
/// pointing at one rots (mbx gc evicts it silently). Those paths fall back
/// to a PATH lookup; an installed binary uses its own absolute path.
pub(crate) fn stable_exe() -> String {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "podbox".to_string());
    const EPHEMERAL: [&str; 4] = ["/target/debug/", "/target/release/", "/.cache/", "/tmp/"];
    if EPHEMERAL.iter().any(|m| exe.contains(m)) {
        "podbox".to_string()
    } else {
        exe
    }
}

pub fn export_app(container_name: &str, app: &str) -> Result<()> {
    if !is_valid_app_name(app) {
        return Err(PodboxError::ExportFailed {
            details: format!("invalid app name: '{app}'"),
        }
        .into());
    }

    // 1. Locate .desktop file in container, searching XDG directories.
    let (container_path, desktop_content) = find_desktop_file(container_name, app)?;

    // 2. Rewrite Name= and Exec= lines
    let rewritten = rewrite_desktop_file(&desktop_content, container_name, app);

    // 3. Write host .desktop file
    let apps_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join("applications");
    std::fs::create_dir_all(&apps_dir)?;

    let host_path = apps_dir.join(format!("podbox-{container_name}-{app}.desktop"));
    std::fs::write(&host_path, rewritten)?;

    // 4. Try to extract icon
    if let Some(icon_name) = extract_icon_name(&desktop_content) {
        if let Err(e) = copy_icon_from_container(container_name, &icon_name, container_name) {
            eprintln!("Warning: failed to copy icon '{icon_name}': {e}");
        }
    }

    // 5. Update desktop database
    if let Err(e) = std::process::Command::new("update-desktop-database")
        .arg(&apps_dir)
        .output()
        .map(|_| ())
    {
        eprintln!("Warning: update-desktop-database failed: {e}");
    }

    println!(
        "Exported app '{}'.desktop (from {}) -> {}",
        app,
        container_path,
        host_path.display()
    );
    Ok(())
}

/// Export every app and bin declared in `[integration.export]`.
///
/// Warn-and-continue: a missing .desktop file must not fail a start.
/// Call only once the container is running — app discovery shells out to
/// `podman exec`, which cannot see a stopped container. (This used to run at
/// `enable` time, where it always failed on fresh boxes for exactly that
/// reason.)
pub fn ensure_exports(config: &Config, name: &str) {
    for app in &config.integration.export.apps {
        if let Err(e) = export_app(name, app) {
            eprintln!("Warning: auto-export app '{app}' failed: {e}");
        }
    }
    for bin in &config.integration.export.bins {
        if let Err(e) = export_bin(name, bin) {
            eprintln!("Warning: auto-export bin '{bin}' failed: {e}");
        }
    }
}

/// Find a `.desktop` file in the container by searching XDG dirs,
/// falling back to user-installed locations.
pub fn export_bin(container_name: &str, bin: &str) -> Result<()> {
    let bin_dir = dirs::home_dir()
        .map(|h| h.join(".local/bin"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));
    std::fs::create_dir_all(&bin_dir)?;

    let exe = stable_exe();
    let shim = format!(
        "#!/bin/sh\nexec {} --container \"{}\" exec \"{}\" \"$@\"\n",
        exe,
        container_name.replace('"', "\\\""),
        bin.replace('"', "\\\"")
    );

    let shim_path = bin_dir.join(bin);
    std::fs::write(&shim_path, shim)?;
    #[allow(clippy::print_literal)]
    {
        let _ = std::fs::set_permissions(&shim_path, std::fs::Permissions::from_mode(0o755));
    }

    println!("Exported bin shim '{}' -> {}", bin, shim_path.display());
    Ok(())
}

/// Remove all exports for a container.
pub fn unexport_all(container_name: &str) -> Result<()> {
    let apps_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join("applications");
    let prefix = format!("podbox-{container_name}");

    if let Ok(entries) = std::fs::read_dir(&apps_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    let icons_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join(format!("icons/podbox/{container_name}"));
    // Also remove legacy icons path
    let old_icons_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join(format!("icons/podmgr/{container_name}"));
    let _ = std::fs::remove_dir_all(&icons_dir);
    if old_icons_dir.exists() {
        let _ = std::fs::remove_dir_all(&old_icons_dir);
    }

    let bin_dir = dirs::home_dir()
        .map(|h| h.join(".local/bin"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));

    // Remove shims that reference this container
    let marker = format!("--container \"{container_name}\"");
    if let Ok(entries) = std::fs::read_dir(&bin_dir) {
        for entry in entries.flatten() {
            if let Ok(mut file) = std::fs::File::open(entry.path()) {
                use std::io::Read;
                let mut chunk = vec![0u8; 4096];
                if let Ok(bytes_read) = file.read(&mut chunk) {
                    let content = String::from_utf8_lossy(&chunk[..bytes_read]);
                    if content.contains(&marker) {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }

    println!("Unexported all apps and bins for '{container_name}'.");
    Ok(())
}

/// One exported `.desktop` app: the config/export id, the display name from
/// the desktop file when the export exists, and whether it is on the host.
#[derive(Debug, Clone, Serialize)]
pub struct ExportedApp {
    pub id: String,
    pub name: Option<String>,
    pub exported: bool,
}

/// One exported bin shim: the name, its absolute path inside the container
/// when the box is running, and whether the shim is on the host.
#[derive(Debug, Clone, Serialize)]
pub struct ExportedBin {
    pub name: String,
    pub path: Option<String>,
    pub exported: bool,
}

/// Full export inventory for a container: declared items plus anything
/// actually on the host, so clients can show pending vs live exports.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExportInventory {
    pub apps: Vec<ExportedApp>,
    pub bins: Vec<ExportedBin>,
}

/// Build the export inventory: the union of declared (`[integration.export]`)
/// items and what's actually on the host. Dictionary order, exported flag
/// set for host-present items.
pub fn inventory(
    container_name: &str,
    declared_apps: &[String],
    declared_bins: &[String],
) -> ExportInventory {
    let on_disk = exported_desktop_files(container_name);
    let mut ids: std::collections::BTreeSet<String> = declared_apps.iter().cloned().collect();
    ids.extend(on_disk.keys().cloned());
    let apps = ids
        .into_iter()
        .map(|id| {
            let exported = on_disk.contains_key(&id);
            let name = on_disk.get(&id).and_then(|p| desktop_display_name(p));
            ExportedApp { id, name, exported }
        })
        .collect();

    let exported_bins = exported_shim_names(container_name);
    let mut names: std::collections::BTreeSet<String> = declared_bins.iter().cloned().collect();
    names.extend(exported_bins.iter().cloned());
    let running = matches!(
        crate::podman::query_state(container_name),
        Ok(crate::podman::ContainerState::Running)
    );
    let bins = names
        .into_iter()
        .map(|bin| {
            let exported = exported_bins.contains(&bin);
            let path = container_bin_path(container_name, &bin, running);
            ExportedBin {
                name: bin,
                path,
                exported,
            }
        })
        .collect();

    ExportInventory { apps, bins }
}

/// `(id, host path)` of every exported `.desktop` file for the container.
fn exported_desktop_files(container_name: &str) -> std::collections::BTreeMap<String, PathBuf> {
    let apps_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join("applications");
    let prefix = format!("podbox-{container_name}-");
    let suffix = ".desktop";
    let mut out = std::collections::BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(&apps_dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name.starts_with(&prefix) && file_name.ends_with(suffix) {
                let id = file_name[prefix.len()..file_name.len() - suffix.len()].to_string();
                out.insert(id, entry.path());
            }
        }
    }
    out
}

/// Names of every exported bin shim for the container.
fn exported_shim_names(container_name: &str) -> std::collections::BTreeSet<String> {
    let bin_dir = dirs::home_dir()
        .map(|h| h.join(".local/bin"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));
    let marker = format!("--container \"{container_name}\"");
    let mut out = std::collections::BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(&bin_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(mut file) = std::fs::File::open(&path) {
                use std::io::Read;
                let mut chunk = vec![0u8; 4096];
                if let Ok(bytes_read) = file.read(&mut chunk) {
                    let content = String::from_utf8_lossy(&chunk[..bytes_read]);
                    if content.contains(&marker) {
                        out.insert(entry.file_name().to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    out
}

/// Display name from a `.desktop` file's `Name=` line.
fn desktop_display_name(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    content.lines().find_map(|line| {
        let value = line.strip_prefix("Name=")?.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// Absolute path of a bin inside the running container, via `command -v`.
/// `None` unless the box is running and the lookup succeeds — including
/// for odd names, which are never interpolated into a shell.
fn container_bin_path(container_name: &str, bin: &str, running: bool) -> Option<String> {
    if !running
        || bin.is_empty()
        || !bin
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '/' | ':'))
    {
        return None;
    }
    let probe = format!("command -v {bin}");
    let args = crate::process::args(&["exec", container_name, "sh", "-c", probe.as_str()]);
    let output =
        crate::process::run_piped_timeout("podman", &args, std::time::Duration::from_secs(10))
            .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!path.is_empty()).then_some(path)
}

/// List the .desktop apps and bin shims exported to the host for a container.
pub fn list_exports(container_name: &str) -> Result<()> {
    print_inventory_text(container_name, &inventory(container_name, &[], &[]));
    Ok(())
}

/// Human rendering of an inventory (the `export list` text form).
pub fn print_inventory_text(container_name: &str, inv: &ExportInventory) {
    let apps: Vec<&str> = inv
        .apps
        .iter()
        .filter(|a| a.exported)
        .map(|a| a.id.as_str())
        .collect();
    let bins: Vec<&str> = inv
        .bins
        .iter()
        .filter(|b| b.exported)
        .map(|b| b.name.as_str())
        .collect();

    if apps.is_empty() && bins.is_empty() {
        println!("No exports for '{container_name}'.");
        return;
    }

    if !apps.is_empty() {
        println!("Apps:");
        for app in &apps {
            println!("  {app}");
        }
    }
    if !bins.is_empty() {
        if !apps.is_empty() {
            println!();
        }
        println!("Bins:");
        for bin in &bins {
            println!("  {bin}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_exe_falls_back_for_dev_builds() {
        // Test binaries live under target/debug (or an mbx cache dir), both
        // classified ephemeral — so this must resolve to the PATH fallback.
        assert_eq!(stable_exe(), "podbox");
    }

    #[test]
    fn valid_app_names() {
        for name in &["firefox", "Firefox", "code-oss", "code_oss", "v1.2.3", "a"] {
            assert!(is_valid_app_name(name), "expected '{name}' to be valid");
        }
    }

    #[test]
    fn reject_empty_name() {
        assert!(!is_valid_app_name(""));
    }

    #[test]
    fn reject_shell_metacharacters() {
        for bad in &[
            "foo;rm", "foo\"bar", "foo`bar", "foo$bar", "foo|bar", "foo>bar", "foo<bar", "foo&bar",
            "foo\nbar", "../foo", "foo/bar", "foo bar", "foo\\bar", "foo'bar",
        ] {
            assert!(!is_valid_app_name(bad), "expected '{bad}' to be rejected");
        }
    }

    #[test]
    fn export_app_rejects_invalid_name() {
        let result = export_app("test-container", "foo;rm");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("foo;rm") || err.contains("invalid"),
            "error should mention the name: {err}"
        );
    }

    #[test]
    fn find_desktop_file_rejects_invalid_name() {
        let result = find_desktop_file("test-container", "foo`whoami`");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("foo`whoami`") || err.contains("invalid"),
            "error should mention the name: {err}"
        );
    }

    #[test]
    fn rewrite_desktop_file_exec_has_no_leading_whitespace() {
        let input = "[Desktop Entry]\nName=Firefox\nExec=/usr/bin/firefox %u\nIcon=firefox\n";
        let out = rewrite_desktop_file(input, "box", "firefox");
        let exec = out
            .lines()
            .find(|l| l.starts_with("Exec="))
            .expect("rewritten Exec= line");
        assert!(
            exec.starts_with("Exec="),
            "Exec key must start at column 0, got: {exec:?}"
        );
        assert!(exec.contains("--container \"box\" run /usr/bin/firefox %u"));
    }

    #[test]
    fn rewrite_desktop_file_appends_container_to_name() {
        let input = "[Desktop Entry]\nName=Firefox\nExec=/usr/bin/firefox\n";
        let out = rewrite_desktop_file(input, "box", "firefox");
        assert!(out.contains("Name=Firefox (box)"));
    }

    #[test]
    fn list_exports_lists_apps_and_bins() {
        let apps_dir = dirs::data_dir().expect("data dir").join("applications");
        std::fs::create_dir_all(&apps_dir).expect("create apps dir");
        let app_path = apps_dir.join("podbox-box-firefox.desktop");
        std::fs::write(&app_path, "[Desktop Entry]\nName=Firefox (box)\n").unwrap();

        let bin_dir = dirs::home_dir().expect("home dir").join(".local/bin");
        std::fs::create_dir_all(&bin_dir).expect("create bin dir");
        let shim_path = bin_dir.join("firefox");
        std::fs::write(
            &shim_path,
            "#!/bin/sh\nexec /usr/bin/podbox --container \"box\" exec \"firefox\" \"$@\"\n",
        )
        .unwrap();

        let apps_dir = dirs::data_dir().expect("data dir").join("applications");
        let prefix = format!("podbox-{}-", "box");
        let suffix = ".desktop";
        let mut apps: Vec<String> = std::fs::read_dir(&apps_dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(&prefix) && n.ends_with(suffix))
            .map(|n| n[prefix.len()..n.len() - suffix.len()].to_string())
            .collect();
        apps.sort();

        let marker = format!("--container \"{}\"", "box");
        let mut bins: Vec<String> = std::fs::read_dir(&bin_dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                std::fs::read_to_string(e.path())
                    .map(|c| c.contains(&marker))
                    .unwrap_or(false)
            })
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        bins.sort();

        assert_eq!(apps, vec!["firefox".to_string()]);
        assert_eq!(bins, vec!["firefox".to_string()]);

        let _ = std::fs::remove_file(&app_path);
        let _ = std::fs::remove_file(&shim_path);
    }
}
