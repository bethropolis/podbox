use anyhow::{Context, Result};
use nix::fcntl::{Flock, FlockArg};

use crate::codegen::quadlet;
use crate::config::Config;
use crate::env::HostEnv;
use crate::podman::{PodmanVersion, podman_version};
use crate::systemd;
use crate::xdg::ResolvedXdgDirs;

use super::paths::{quadlet_dir, systemd_user_dir};
use super::preflight::preflight_check;
use super::transaction::{restore_prior_units, snapshot_prior_units};
use super::units::{
    finalize_units, podman_quadlet_install_application, podman_quadlet_install_files,
    remove_application_dir, remove_flat_units,
};
use super::{preflight, targets};

/// Install systemd service and socket files for a container.
pub fn install(config: &Config, env: &HostEnv, xdg: &ResolvedXdgDirs, dry_run: bool) -> Result<()> {
    install_with_ctx(
        config,
        env,
        xdg,
        &crate::ui::Ctx::new(false, false, dry_run),
    )
}

/// Same as [`install`], with the caller's confirmation context (`--yes`
/// skips preflight prompts in unattended installs).
pub fn install_with_ctx(
    config: &Config,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
    ctx: &crate::ui::Ctx,
) -> Result<()> {
    let dry_run = ctx.dry_run;
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
    preflight_check(config, ctx)?;

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
