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
    rewrite_desktop_file, rewrite_host_icon,
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
    crate::fsutil::write_atomic(&host_path, rewritten.as_bytes())?;

    // 4. Try to extract the icon, and point the launcher at the copy.
    // The copy lands outside the theme search path, so the container-side
    // Icon name/path would otherwise show nothing.
    if let Some(icon_name) = extract_icon_name(&desktop_content) {
        match copy_icon_from_container(container_name, &icon_name, container_name) {
            Ok(Some(dest)) => {
                if let Err(e) = rewrite_host_icon(&host_path, &dest) {
                    eprintln!(
                        "Warning: failed to point Icon at '{}': {e:#}",
                        dest.display()
                    );
                }
            }
            Ok(None) => {}
            Err(e) => eprintln!("Warning: failed to copy icon '{icon_name}': {e:#}"),
        }
    }

    // 5. Update desktop database
    let args = crate::process::args(&[apps_dir.to_string_lossy().to_string()]);
    if let Ok(output) = crate::process::run_piped("update-desktop-database", &args)
        && !output.status.success()
    {
        eprintln!("Warning: update-desktop-database exited {}", output.status);
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

/// Validate a binary name before it reaches a host path or an argv.
fn validate_bin_name(bin: &str) -> Result<()> {
    if bin.is_empty()
        || bin == "."
        || bin == ".."
        || bin.starts_with('-')
        || bin.contains('/')
        || bin.contains('\\')
        || bin.contains("..")
        || bin.chars().any(char::is_whitespace)
    {
        return Err(PodboxError::ExportFailed {
            details: format!("invalid bin name: '{bin}'"),
        }
        .into());
    }
    Ok(())
}

/// POSIX single-quote escaping: inside single quotes only `'` needs special
/// handling, and `$`, backticks and backslashes stay inert.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Container named by a `podbox`/`podmgr` command line, if the program is
/// ours and it carries `--container`/`-C`.
fn podbox_container_from_argv(line: &str) -> Option<String> {
    let args = shell_words::split(line).ok()?;
    let mut idx = 0;
    // Skip `exec` and leading `KEY=value` assignments.
    while idx < args.len() && (args[idx] == "exec" || args[idx].contains('=')) {
        idx += 1;
    }
    let program = Path::new(args.get(idx)?).file_name()?.to_str()?;
    if program != "podbox" && program != "podmgr" {
        return None;
    }
    let pos = args.iter().position(|a| a == "--container" || a == "-C")?;
    args.get(pos + 1).cloned()
}

/// Container referenced by a `.desktop` file's `Exec=` line, if it is a
/// podbox launcher.
pub fn desktop_exec_container(content: &str) -> Option<String> {
    let exec = content.lines().find_map(|l| l.strip_prefix("Exec="))?;
    podbox_container_from_argv(exec)
}

/// Container referenced by a bin shim script, if it invokes podbox/podmgr.
pub fn shim_exec_container(content: &str) -> Option<String> {
    content
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .find_map(podbox_container_from_argv)
}

/// Read at most `limit` bytes, as UTF-8.
fn read_prefix(path: &Path, limit: usize) -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; limit];
    let n = file.read(&mut buf).ok()?;
    String::from_utf8(buf[..n].to_vec()).ok()
}

/// Export a container binary as a host shim.
pub fn export_bin(container_name: &str, bin: &str) -> Result<()> {
    validate_bin_name(bin)?;
    let bin_dir = dirs::home_dir()
        .map(|h| h.join(".local/bin"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));
    std::fs::create_dir_all(&bin_dir)?;

    let exe = stable_exe();
    let shim = format!(
        "#!/bin/sh\nexec {} --container {} exec -- {} \"$@\"\n",
        shell_quote(&exe),
        shell_quote(container_name),
        shell_quote(bin)
    );

    let shim_path = bin_dir.join(bin);
    crate::fsutil::write_atomic(&shim_path, shim.as_bytes())?;
    #[allow(clippy::print_literal)]
    {
        let _ = std::fs::set_permissions(&shim_path, std::fs::Permissions::from_mode(0o755));
    }

    println!("Exported bin shim '{}' -> {}", bin, shim_path.display());
    Ok(())
}

/// Remove one exported app launcher from the host.
///
/// Errors when nothing with that id is exported, so scripts can tell
/// "already gone" apart from success.
pub fn unexport_app(container_name: &str, app: &str) -> Result<()> {
    if !is_valid_app_name(app) {
        return Err(PodboxError::ExportFailed {
            details: format!("invalid app name: '{app}'"),
        }
        .into());
    }
    let path = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join("applications")
        .join(format!("podbox-{container_name}-{app}.desktop"));
    let owned = read_prefix(&path, 4096)
        .and_then(|content| desktop_exec_container(&content))
        .is_some_and(|name| name == container_name);
    if !path.exists() || !owned {
        return Err(PodboxError::ExportFailed {
            details: format!("no exported app '{app}' for '{container_name}'"),
        }
        .into());
    }
    std::fs::remove_file(&path)?;
    println!("Unexported app '{app}' for '{container_name}'.");
    Ok(())
}

/// Remove one exported bin shim from the host.
///
/// Only removes shims that reference this container, never a same-named
/// host binary. Errors when no such shim exists.
pub fn unexport_bin(container_name: &str, bin: &str) -> Result<()> {
    validate_bin_name(bin)?;
    let path = dirs::home_dir()
        .map(|h| h.join(".local/bin"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"))
        .join(bin);
    let owned = read_prefix(&path, 4096)
        .and_then(|content| shim_exec_container(&content))
        .is_some_and(|name| name == container_name);
    if !owned {
        return Err(PodboxError::ExportFailed {
            details: format!("no exported bin '{bin}' for '{container_name}'"),
        }
        .into());
    }
    std::fs::remove_file(&path)?;
    println!("Unexported bin '{bin}' for '{container_name}'.");
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
    let prefix = format!("podbox-{container_name}-");

    if let Ok(entries) = std::fs::read_dir(&apps_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            // Trailing dash: without it, cleaning `box` would also delete
            // `box2`'s launchers. Verify the Exec target too, because a
            // hyphenated container (`dev-rust`) shares the `podbox-dev-`
            // prefix with `dev`.
            if name.to_string_lossy().starts_with(&prefix)
                && read_prefix(&entry.path(), 4096)
                    .and_then(|content| desktop_exec_container(&content))
                    .is_some_and(|container| container == container_name)
            {
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
    if let Ok(entries) = std::fs::read_dir(&bin_dir) {
        for entry in entries.flatten() {
            if read_prefix(&entry.path(), 4096)
                .and_then(|content| shim_exec_container(&content))
                .is_some_and(|name| name == container_name)
            {
                let _ = std::fs::remove_file(entry.path());
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
    exported_desktop_files_in(&apps_dir, container_name)
}

/// [`exported_desktop_files`] against an explicit directory (testable).
fn exported_desktop_files_in(
    apps_dir: &Path,
    container_name: &str,
) -> std::collections::BTreeMap<String, PathBuf> {
    let prefix = format!("podbox-{container_name}-");
    let suffix = ".desktop";
    let mut out = std::collections::BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(apps_dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if !(file_name.starts_with(&prefix) && file_name.ends_with(suffix)) {
                continue;
            }
            // The prefix is not enough: `dev` and `dev-rust` share it.
            let owned = read_prefix(&entry.path(), 4096)
                .and_then(|content| desktop_exec_container(&content))
                .is_some_and(|name| name == container_name);
            if owned {
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
    exported_shim_names_in(&bin_dir, container_name)
}

/// [`exported_shim_names`] against an explicit directory (testable).
fn exported_shim_names_in(
    bin_dir: &Path,
    container_name: &str,
) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(bin_dir) {
        for entry in entries.flatten() {
            let owned = read_prefix(&entry.path(), 4096)
                .and_then(|content| shim_exec_container(&content))
                .is_some_and(|name| name == container_name);
            if owned {
                out.insert(entry.file_name().to_string_lossy().into_owned());
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
    let args = crate::process::args(&[
        "exec",
        container_name,
        "sh",
        "-c",
        "command -v -- \"$1\"",
        "podbox-probe",
        bin,
    ]);
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
    fn shell_quote_keeps_metacharacters_inert() {
        assert_eq!(shell_quote("box"), "'box'");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
        assert_eq!(shell_quote("$(id)`whoami`\\$x"), "'$(id)`whoami`\\$x'");
    }

    #[test]
    fn export_bin_rejects_path_traversal() {
        for bad in [
            "",
            ".",
            "..",
            "../.bashrc",
            "sub/bin",
            "a..b",
            "/etc/passwd",
            "-weird",
            "a b",
        ] {
            let err = match validate_bin_name(bad) {
                Ok(()) => panic!("'{bad}' must be rejected"),
                Err(err) => err,
            };
            assert!(err.to_string().contains("invalid bin name"));
        }
        assert!(validate_bin_name("firefox").is_ok());
    }

    #[test]
    fn shim_marker_requires_podbox_and_container_flag() {
        assert_eq!(
            shim_exec_container(
                "#!/bin/sh\nexec podbox --container \"box\" exec \"firefox\" \"$@\"\n"
            )
            .as_deref(),
            Some("box")
        );
        assert_eq!(
            shim_exec_container(
                "#!/bin/sh\nexec '/usr/bin/podbox' --container 'box' exec -- 'firefox' \"$@\"\n"
            )
            .as_deref(),
            Some("box")
        );
        assert_eq!(
            shim_exec_container("#!/bin/sh\ngit -C \"$HOME/work\" status\n"),
            None
        );
        assert_eq!(
            shim_exec_container("#!/bin/sh\npodman --container foo ps\n"),
            None
        );
    }

    #[test]
    fn desktop_marker_requires_podbox() {
        assert_eq!(
            desktop_exec_container(
                "[Desktop Entry]\nExec=podbox --container \"box\" run /usr/bin/firefox %u\n"
            )
            .as_deref(),
            Some("box")
        );
        assert_eq!(
            desktop_exec_container("[Desktop Entry]\nExec=/usr/bin/git -C \"$HOME/work\" status\n"),
            None
        );
    }

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
    fn exported_inventory_ignores_hyphenated_siblings_and_foreign_bins() {
        let root = tempfile::tempdir().unwrap();
        let apps = root.path().join("applications");
        let bin = root.path().join("bin");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::create_dir_all(&bin).unwrap();

        std::fs::write(
            apps.join("podbox-dev-firefox.desktop"),
            "[Desktop Entry]\nExec=podbox --container \"dev\" run /usr/bin/firefox\n",
        )
        .unwrap();
        std::fs::write(
            apps.join("podbox-dev-rust-code.desktop"),
            "[Desktop Entry]\nExec=podbox --container \"dev-rust\" run /usr/bin/code\n",
        )
        .unwrap();
        std::fs::write(
            bin.join("firefox"),
            "#!/bin/sh\nexec podbox --container 'dev' exec -- 'firefox' \"$@\"\n",
        )
        .unwrap();
        std::fs::write(bin.join("git"), "#!/bin/sh\ngit -C \"$HOME/work\" status\n").unwrap();

        let apps_found = exported_desktop_files_in(&apps, "dev");
        assert_eq!(
            apps_found.keys().cloned().collect::<Vec<_>>(),
            vec!["firefox".to_string()]
        );
        let bins_found = exported_shim_names_in(&bin, "dev");
        assert_eq!(
            bins_found.into_iter().collect::<Vec<_>>(),
            vec!["firefox".to_string()]
        );
    }
}
