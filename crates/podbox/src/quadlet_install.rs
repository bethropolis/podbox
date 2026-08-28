//! Quadlet/systemd install and uninstall for a container definition.
//!
//! Slim dispatcher module; path discovery lives in [`paths`], unit writing in
//! [`units`], and pre-install validation in [`preflight`].

pub mod paths;
pub mod preflight;
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

/// Install systemd service and socket files for a container.
pub fn install(config: &Config, env: &HostEnv, xdg: &ResolvedXdgDirs, dry_run: bool) -> Result<()> {
    let name = &config.container.name;
    let ver = podman_version().unwrap_or(PodmanVersion {
        major: 5,
        minor: 5,
        patch: 0,
    });
    let qdir = quadlet_dir();
    let sdir = systemd_user_dir();
    let context_dir = crate::build::build_context_dir(name);
    let containerfile_path = context_dir.join("Containerfile");

    let socket_content = quadlet::generate_socket(config);
    let container_content = quadlet::generate_container(config, env, xdg);
    let host_service_content = quadlet::generate_host_service(name);
    let dbus_proxy_content = quadlet::generate_dbus_proxy_service(name, config);
    let compositor_service_content = quadlet::generate_compositor_service(name, config);

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
        return Ok(());
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
    std::fs::write(
        context_dir.join(".flatpak-info"),
        "[Application]\nname=podbox\n",
    )?;

    // Pre-flight validation
    preflight_check(config)?;

    // Ensure home and runtime dirs exist
    std::fs::create_dir_all(&config.container.home).with_context(|| {
        format!(
            "failed to create home dir '{}'",
            config.container.home.display()
        )
    })?;

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
            std::fs::write(qdir.join(format!("{name}.build")), bc)?;
        }
        std::fs::write(qdir.join(format!("{name}.container")), container_content)?;
        println!("Quadlet files installed to {}", qdir.display());
    }

    finalize_units(
        name,
        &sdir,
        &socket_content,
        &host_service_content,
        dbus_proxy_content.as_deref(),
        compositor_service_content.as_deref(),
        config.use_wayland_proxy(),
    )?;

    // Auto-export apps and bins
    for app in &config.integration.export.apps {
        if let Err(e) = crate::export::export_app(name, app) {
            eprintln!("Warning: auto-export app '{app}' failed: {e}");
        }
    }
    for bin in &config.integration.export.bins {
        if let Err(e) = crate::export::export_bin(name, bin) {
            eprintln!("Warning: auto-export bin '{bin}' failed: {e}");
        }
    }

    if config.lifecycle.autostart {
        systemd::enable_linger()?;
    }

    Ok(())
}

/// Remove Quadlet and systemd files for a container.
pub fn uninstall(name: &str) -> Result<()> {
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

    // Remove custom systemd units
    for unit in [
        "socket",
        "host.service",
        "proxy.service",
        "compositor.service",
    ] {
        let path = sdir.join(format!("{name}.{unit}"));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    }

    // Remove the clean-stop drop-in directory for the generated container unit.
    let dropin_dir = sdir.join(format!("{name}.service.d"));
    if dropin_dir.is_dir() {
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
