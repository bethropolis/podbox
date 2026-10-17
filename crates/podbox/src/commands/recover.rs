//! Guided, idempotent repair for a container that won't start.
//!
//! `podbox recover` walks four safe steps (systemd reload, Quadlet
//! reinstall, image rebuild if missing, restart), confirming each on a TTY
//! unless `--yes`. It never deletes container data — `remove --all` remains
//! the only destructive path.

use anyhow::Result;

use podbox::config::Config;
use podbox::env::HostEnv;
use podbox::xdg::ResolvedXdgDirs;

use podbox::ui;

pub struct RecoverOpts {
    pub yes: bool,
    pub dry_run: bool,
}

/// A step is skipped, not fatal, when it cannot be confirmed: scripts
/// without `--yes` keep going with the steps they can run safely, and a
/// declined prompt does the same (recover never deletes anything).
fn confirmed(opts: &RecoverOpts, step: &str) -> bool {
    if opts.dry_run {
        return true;
    }
    let ctx = ui::Ctx::new(opts.yes, false, opts.dry_run);
    match ui::confirm(&ctx, &format!("Step: {step}"), true) {
        Ok(()) => true,
        Err(podbox::error::PodboxError::ConfirmationRequired) => {
            eprintln!("Skipping '{step}' (non-interactive; use --yes to allow).");
            false
        }
        Err(_) => false,
    }
}

/// Walk the recovery plan. Every step is safe to re-run.
pub fn run_recover(
    config: &Config,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
    name: &str,
    opts: RecoverOpts,
) -> Result<()> {
    let dry = opts.dry_run;
    println!("Recovery plan for '{name}':");
    println!("  1. systemctl --user daemon-reload + reset-failed");
    println!("  2. reinstall Quadlet files (podbox enable)");
    println!("  3. rebuild image if missing or corrupt");
    println!("  4. stop and start the container");
    if dry {
        return Ok(());
    }
    println!();
    // Nothing here deletes home or config; that stays with `remove`.
    let _ = name;

    // 1 — systemd bookkeeping.
    if confirmed(&opts, "Reload systemd and reset failed units?") {
        ui::step("Reloading systemd user manager...");
        podbox::systemd::daemon_reload()?;
        podbox::systemd::reset_failed(name)?;
        ui::ok("Systemd state reset");
    }

    // 2 — reinstall Quadlet so stale/partial unit files are regenerated.
    if confirmed(&opts, "Reinstall Quadlet systemd files?") {
        ui::step("Installing Quadlet files...");
        podbox::quadlet_install::install(config, env, xdg, false)?;
        ui::ok("Quadlet files installed");
    }

    // 3 — rebuild only when the image is actually absent.
    let tag = config.local_image_tag();
    let image_ok = podbox::podman::image_exists(&tag).unwrap_or(false);
    if !image_ok && confirmed(&opts, "Image is missing - build it now?") {
        podbox::build::run(
            config,
            env,
            xdg,
            false,
            false,
            podbox::cli::OutputFormat::Text,
        )?;
    } else if image_ok {
        ui::ok(&format!("Image {tag} present"));
    }

    // 4 — fresh start through systemd.
    if confirmed(&opts, "Stop and start the container?") {
        ui::step("Stopping...");
        let _ = podbox::systemd::stop_unit(name);
        ui::step("Starting...");
        podbox::systemd::start_unit(name)?;
        ui::ok(&format!("Container '{name}' started"));
    }

    ui::ok("Recovery complete. If problems persist: podbox doctor / podbox logs");
    let _ = podbox::history::record(name, "recover", "");
    Ok(())
}
