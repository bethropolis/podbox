use anyhow::Result;

use crate::podman::{PodmanVersion, podman_version};
use crate::systemd;

use super::paths::{quadlet_dir, systemd_user_dir};
use super::units::{self, remove_application_dir};

/// Remove Quadlet and systemd files for a container.
pub fn uninstall(name: &str) -> Result<()> {
    // Disable first: `enable --now` leaves enabled symlinks behind, and
    // deleting the unit files without disabling orphans them — the socket
    // stays enabled across daemon-reloads. Best-effort: the units may never
    // have been enabled, and file cleanup must proceed regardless.
    if systemd::is_available() {
        let unit_names = [
            format!("{name}.socket"),
            format!("{name}.service"),
            format!("{name}-host.service"),
            format!("{name}-proxy.service"),
            format!("{name}-compositor.service"),
            format!("{name}-x11.service"),
        ];
        let mut args: Vec<std::ffi::OsString> = vec!["--user".into(), "disable".into()];
        args.extend(unit_names.iter().map(std::ffi::OsString::from));
        match crate::process::run_piped("systemctl", &args) {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!(
                    "Warning: systemctl disable for '{name}' failed: {}",
                    stderr.trim()
                );
            }
            Err(e) => eprintln!("Warning: systemctl disable for '{name}' failed: {e:#}"),
        }
    }

    let ver = podman_version().unwrap_or(PodmanVersion {
        major: 5,
        minor: 5,
        patch: 0,
    });
    let qdir = quadlet_dir();
    let sdir = systemd_user_dir();

    if ver.at_least(5, 6) {
        let mut removed_via_podman = false;

        // Flat units: only call rm when the file exists (avoids needing --ignore).
        for ext in ["container", "build"] {
            let path = qdir.join(format!("{name}.{ext}"));
            if !path.exists() {
                continue;
            }
            let args: Vec<std::ffi::OsString> = vec![
                "quadlet".into(),
                "rm".into(),
                format!("{name}.{ext}").into(),
            ];
            let output = crate::process::run_piped("podman", &args)?;
            if output.status.success() {
                removed_via_podman = true;
            } else {
                // Fall through to manual delete below.
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("Warning: podman quadlet rm {name}.{ext} failed: {stderr}");
            }
        }

        // Application-scoped leftovers (directory install / --application).
        let app_dir = qdir.join(name);
        if app_dir.is_dir() {
            if ver.at_least(6, 0) {
                let args: Vec<std::ffi::OsString> = vec![
                    "quadlet".into(),
                    "rm".into(),
                    "--recursive".into(),
                    name.into(),
                ];
                let output = crate::process::run_piped("podman", &args)?;
                if output.status.success() {
                    removed_via_podman = true;
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("Warning: podman quadlet rm --recursive {name} failed: {stderr}");
                }
            }
            remove_application_dir(name);
        }

        // Manual cleanup of any remaining flat files.
        for ext in ["build", "container"] {
            let path = qdir.join(format!("{name}.{ext}"));
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }

        if removed_via_podman {
            println!("Quadlet files removed via podman quadlet rm.");
        }
    } else {
        // 5.5 fallback: remove files manually
        for ext in ["build", "container"] {
            let path = qdir.join(format!("{name}.{ext}"));
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        remove_application_dir(name);
    }

    // Remove custom systemd units. Driven by the same filename list the
    // installer writes, so a unit cannot be created but not removed.
    for file in units::custom_unit_filenames(name) {
        let path = sdir.join(&file);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    }

    // Remove the clean-stop drop-in directory for the generated container unit.
    let dropin_dir = sdir.join(format!("{name}.service.d"));
    if dropin_dir.is_dir() {
        anyhow::ensure!(
            units::is_within(&dropin_dir, &sdir),
            "refusing to remove '{}': it resolves outside {}",
            dropin_dir.display(),
            sdir.display()
        );
        std::fs::remove_dir_all(&dropin_dir)?;
    }

    systemd::daemon_reload()?;
    println!("Files for '{name}' removed.");

    Ok(())
}
