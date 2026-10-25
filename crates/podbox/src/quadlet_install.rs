//! Quadlet/systemd install and uninstall for a container definition.
//!
//! Slim dispatcher module; path discovery lives in [`paths`], unit writing in
//! [`units`], and pre-install validation in [`preflight`].

pub mod paths;
pub mod preflight;
pub mod targets;
pub mod units;

pub use paths::{is_installed, list_installed_names};

use anyhow::{Context, Result};
use nix::fcntl::{Flock, FlockArg};

use crate::codegen::quadlet;
use crate::config::Config;
use crate::env::HostEnv;
use crate::podman::{PodmanVersion, podman_version};
use crate::systemd;
use crate::xdg::ResolvedXdgDirs;

use paths::{quadlet_dir, systemd_user_dir};
use preflight::preflight_check;
use units::{
    finalize_units, podman_quadlet_install_application, podman_quadlet_install_files,
    remove_application_dir, remove_flat_units,
};

/// A Quadlet/systemd file as it was before an install: its path and its
/// prior bytes, or `None` when the file did not exist (so rollback removes
/// a half-written new file instead of leaving it behind).
struct PriorFile {
    path: std::path::PathBuf,
    bytes: Option<Vec<u8>>,
}

/// Snapshot every unit file an install may replace, so a failure can put
/// the previous state back.
fn snapshot_prior_units(
    name: &str,
    qdir: &std::path::Path,
    sdir: &std::path::Path,
) -> Vec<PriorFile> {
    let mut paths = vec![
        qdir.join(format!("{name}.container")),
        qdir.join(format!("{name}.build")),
        qdir.join(name).join(format!("{name}.container")),
        qdir.join(name).join(format!("{name}.build")),
    ];
    paths.extend(
        units::custom_unit_filenames(name)
            .iter()
            .map(|f| sdir.join(f)),
    );
    paths
        .into_iter()
        .filter_map(|path| match std::fs::read(&path) {
            Ok(bytes) => Some(PriorFile {
                path,
                bytes: Some(bytes),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(PriorFile {
                path,
                bytes: None,
            }),
            Err(e) => {
                eprintln!(
                    "Warning: cannot snapshot '{}' for rollback ({e:#}); it will be left alone on failure",
                    path.display()
                );
                None
            }
        })
        .collect()
}

/// Best-effort restore of a [`snapshot_prior_units`] snapshot. Returns the
/// number of files that could not be restored.
fn restore_prior_units(prior: &[PriorFile]) -> usize {
    let mut failed = 0usize;
    for file in prior {
        let result = match &file.bytes {
            Some(bytes) => {
                if let Some(parent) = file.path.parent() {
                    if std::fs::create_dir_all(parent).is_err() {
                        Err(())
                    } else {
                        crate::fsutil::write_atomic(&file.path, bytes).map_err(|_| ())
                    }
                } else {
                    crate::fsutil::write_atomic(&file.path, bytes).map_err(|_| ())
                }
            }
            None => {
                if file.path.exists() {
                    std::fs::remove_file(&file.path).map_err(|_| ())
                } else {
                    Ok(())
                }
            }
        };
        if result.is_err() {
            eprintln!(
                "Warning: rollback could not restore '{}'",
                file.path.display()
            );
            failed += 1;
        }
    }
    failed
}

/// Install systemd service and socket files for a container.
pub fn install(config: &Config, env: &HostEnv, xdg: &ResolvedXdgDirs, dry_run: bool) -> Result<()> {
    let name = &config.container.name;
    let x11_dependencies = preflight::resolve_x11_dependencies(config)?;
    let ver = podman_version().unwrap_or(PodmanVersion {
        major: 5,
        minor: 5,
        patch: 0,
    });
    let qdir = quadlet_dir();
    let sdir = systemd_user_dir();
    let context_dir = crate::env::build_context_dir(name);
    let containerfile_path = context_dir.join("Containerfile");

    let socket_content = quadlet::generate_socket(config);
    let container_content = quadlet::generate_container(config, env, xdg);
    let host_service_content = quadlet::generate_host_service(name);
    let dbus_proxy_content = quadlet::generate_dbus_proxy_service(name, config);
    let compositor_service_content = quadlet::generate_compositor_service(name, config);
    let x11_service_content = if let Some(deps) = &x11_dependencies {
        let display = env
            .wayland_display
            .as_deref()
            .context("x11 requires an active WAYLAND_DISPLAY on the host")?;
        Some(quadlet::generate_x11_service(
            config,
            &deps.satellite.to_string_lossy(),
            &quadlet::wayland_socket_path(config, display, name),
        ))
    } else {
        None
    };

    let build_content = if !config.image.source().is_prebuilt() {
        Some(quadlet::generate_build(config, &containerfile_path))
    } else {
        None
    };

    if dry_run {
        if let Some(ref bc) = build_content {
            println!("=== {name}.build ===");
            println!("{bc}");
            println!();
        }
        println!("=== {name}.socket ===");
        println!("{socket_content}");
        println!();
        println!("=== {name}.container ===");
        println!("{container_content}");
        println!();
        println!("=== {name}-host.service ===");
        println!("{host_service_content}");
        if let Some(ref proxy) = dbus_proxy_content {
            println!();
            println!("=== {name}-proxy.service ===");
            println!("{proxy}");
        }
        if let Some(ref comp) = compositor_service_content {
            println!();
            println!("=== {name}-compositor.service ===");
            println!("{comp}");
        }
        if let Some(ref x11) = x11_service_content {
            println!();
            println!("=== {name}-x11.service ===");
            println!("{x11}");
        }
        return Ok(());
    }

    // Unit files embed the invoking binary. Warn when it is a dev build
    // (target/, cache): those paths rot on rebuild or gc, and the fallback
    // is silent — services keep running stale code.
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "podbox".into());
    let stable = crate::export::stable_exe();
    if exe != stable {
        eprintln!(
            "Warning: enabling with a dev binary ({exe}); generated units will reference it. \
             Prefer the installed binary ({stable}) so services survive rebuilds."
        );
    }

    // Acquire exclusive install lock (auto-releases on panic/crash via kernel flock)
    let _install_lock = {
        let lock_path = context_dir.join(".install.lock");
        let _ = std::fs::create_dir_all(&context_dir);
        let file = std::fs::File::create(&lock_path).with_context(|| {
            format!("failed to create install lock at '{}'", lock_path.display())
        })?;
        Flock::lock(file, FlockArg::LockExclusive).map_err(|(_, e)| e)?
    };

    // Ensure .flatpak-info is written to the host build directory
    let _ = std::fs::create_dir_all(&context_dir);
    crate::fsutil::write_atomic(
        &context_dir.join(".flatpak-info"),
        b"[Application]\nname=podbox\n",
    )?;

    // Pre-flight validation. Confirmation state mirrors the install call:
    // dry runs print instead of validating ahead, and callers that carry
    // `--yes` use it to skip prompts inside the preflight.
    preflight_check(config, &crate::ui::Ctx::new(false, false, dry_run))?;

    // Ensure home and runtime dirs exist
    std::fs::create_dir_all(&config.container.home).with_context(|| {
        format!(
            "failed to create home dir '{}'",
            config.container.home.display()
        )
    })?;

    // Pre-create bind-mount targets inside the home with the host user's
    // ownership, so podman never creates them as root (see targets.rs).
    targets::precreate_bind_targets(config, env, xdg)?;

    // Snapshot the current unit files before touching Quadlet state: the
    // steps below delete the old layout first (flat units on the Podman 6
    // path), so without a snapshot a failed install leaves a half-written
    // state behind with no way back.
    let prior = snapshot_prior_units(name, &qdir, &sdir);

    let install_result = (|| -> Result<()> {
        if ver.at_least(6, 0) {
            // 6.0+: use --application with directory install.
            remove_flat_units(name);
            podman_quadlet_install_application(name, &container_content, build_content.as_deref())?;
        } else if ver.at_least(5, 6) {
            // 5.6–5.x: install individual files for flat layout.
            remove_application_dir(name);
            podman_quadlet_install_files(name, &container_content, build_content.as_deref())?;
        } else {
            // 5.5 fallback: copy files manually
            std::fs::create_dir_all(&qdir)?;
            if let Some(ref bc) = build_content {
                crate::fsutil::write_atomic(&qdir.join(format!("{name}.build")), bc.as_bytes())?;
            }
            crate::fsutil::write_atomic(
                &qdir.join(format!("{name}.container")),
                container_content.as_bytes(),
            )?;
            println!("Quadlet files installed to {}", qdir.display());
        }

        finalize_units(
            name,
            &sdir,
            &socket_content,
            &host_service_content,
            dbus_proxy_content.as_deref(),
            compositor_service_content.as_deref(),
            x11_service_content.as_deref(),
            config.use_wayland_proxy(),
        )?;
        Ok(())
    })();

    if let Err(e) = install_result {
        // Roll back to the snapshotted units so the box keeps whatever
        // worked before instead of a half-installed layout.
        let unrestored = restore_prior_units(&prior);
        let _ = systemd::daemon_reload();
        if unrestored == 0 {
            return Err(e.context("install failed; previous units were restored"));
        }
        return Err(e.context(
            "install failed; rollback was attempted but some previous units could not be restored",
        ));
    }

    // Auto-export runs at start time (see `ensure_exports`), not here:
    // app discovery execs into the container, which may not exist yet at
    // enable time — exporting here only produced bogus warnings on fresh
    // boxes. Bins are host-side shims and could run here, but keeping one
    // call site keeps the behavior uniform.

    if config.lifecycle.autostart {
        systemd::enable_linger()?;
    }

    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::paths::{application_container_path, flat_container_path};

    #[test]
    fn flat_and_application_paths_differ() {
        let flat = flat_container_path("myenv");
        let app = application_container_path("myenv");
        assert!(flat.to_string_lossy().ends_with("myenv.container"));
        assert!(
            app.to_string_lossy().ends_with("myenv/myenv.container")
                || app.to_string_lossy().ends_with("myenv\\myenv.container")
        );
        assert_ne!(flat, app);
    }
}
