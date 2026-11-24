//! Image updates and rollback to a previous checkpoint.

use anyhow::Result;

use podbox::config::Config;
use podbox::env::HostEnv;
use podbox::systemd;
use podbox::xdg::ResolvedXdgDirs;

use super::checkpoint::checkpoint_previous;
use crate::commands::runner::Runtime;

/// Update a container: pull latest image, rebuild, and restart.
pub fn run_update(
    config: &Config,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
    name: &str,
    dry_run: bool,
    no_restart: bool,
) -> Result<()> {
    if dry_run {
        if config.lifecycle.auto_checkpoint {
            checkpoint_previous(config, name, true)?;
        }
        println!("podbox update: pull/rebuild and restart {name}");
        println!("  build::run(config, env, xdg, dry_run: true, rebuild: true)");
        if !no_restart {
            println!(
                "  {}",
                Runtime::for_config(config).describe("restart", name)
            );
        }
        return Ok(());
    }

    println!("Updating '{name}'...");

    if config.lifecycle.auto_checkpoint {
        checkpoint_previous(config, name, false)?;
    }

    podbox::build::run(
        config,
        env,
        xdg,
        false,
        true,
        podbox::cli::OutputFormat::Text,
    )?;

    if no_restart {
        println!("Image updated. Restart skipped (--no-restart).");
        return Ok(());
    }

    println!("Restarting container...");
    let ctx = crate::ctx::Ctx { dry_run: false };
    Runtime::for_config(config).restart(&ctx, config)?;

    println!("Update complete.");
    let _ = podbox::history::record(name, "update", "");
    Ok(())
}

/// Reinstall the Quadlet with the previous image and restart the container.
pub fn run_rollback(
    config: &Config,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
    name: &str,
    dry_run: bool,
) -> Result<()> {
    if !dry_run && (!podbox::quadlet_install::is_installed(name) || !systemd::is_available()) {
        anyhow::bail!("rollback requires an installed, systemd-managed Quadlet container");
    }
    let image = format!("{}:checkpoint-prev", config.image_repo());
    if !dry_run && !podbox::podman::image_exists(&image).unwrap_or(false) {
        anyhow::bail!("no previous checkpoint exists for '{name}' ({image})");
    }
    let mut rollback_config = config.clone();
    rollback_config.image.image_ref = Some(image.clone());
    rollback_config.image.packages.install.clear();
    if dry_run {
        println!("Quadlet Image={image}");
        println!("systemctl --user daemon-reload && systemctl --user restart {name}");
        return Ok(());
    }
    podbox::quadlet_install::install(&rollback_config, env, xdg, false)?;
    systemd::reset_failed(name)?;
    systemd::restart_unit(name)?;
    println!("Rolled '{name}' back to {image}. Container home and TOML were unchanged.");
    Ok(())
}
