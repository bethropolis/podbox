//! Container removal and stale runtime cleanup.

use anyhow::{Context, Result};

use podbox::config::Config;
use podbox::systemd;

/// Remove a container and optionally its home directory.
pub fn run_remove(
    config: &Config,
    name: &str,
    dry_run: bool,
    home: bool,
    force: bool,
    remove_config: bool,
) -> Result<()> {
    if dry_run {
        println!("podman stop {name}");
        println!("podman rm -f {name}");
        if config.lifecycle.quadlet {
            println!("quadlet_install::uninstall({name})");
            println!("systemctl --user reset-failed {name}.service");
        }
        if remove_config {
            let p = podbox::config::find_config_path(name)
                .unwrap_or_else(|| podbox::config::profiles_dir().join(format!("{name}.toml")));
            println!("rm {}", p.display());
        }
        if home {
            println!("rm -rf {}", config.container.home.display());
        }
        return Ok(());
    }

    if !force {
        let ctx = podbox::ui::Ctx::new(false, force, dry_run);
        if let Err(e) = podbox::ui::confirm(&ctx, &format!("Remove container '{name}'?"), false) {
            return Err(e.into());
        }
    }

    // 1. Stop and remove the podman container (best-effort)
    // `--ignore`: a container that was never created (or already gone) is
    // not a failure here — removal proceeds to rm, which is also --ignore.
    if let Err(e) = podbox::process::run_checked(
        "podman",
        &podbox::process::args(&["stop", "--ignore", name]),
    ) {
        eprintln!("Warning: failed to stop container '{name}': {e}");
    }
    // `--ignore`: a container that was never created is not an error here.
    if let Err(e) = podbox::process::run_checked(
        "podman",
        &podbox::process::args(&["rm", "-f", "--ignore", name]),
    ) {
        eprintln!("Warning: failed to remove container '{name}': {e}");
    }

    // 2. Clean up Quadlet files and systemd units
    if config.lifecycle.quadlet {
        if let Err(e) = systemd::stop_unit(name) {
            eprintln!("Warning: failed to stop systemd unit '{name}': {e}");
        }
        if let Err(e) = podbox::quadlet_install::uninstall(name) {
            eprintln!("Warning: failed to uninstall Quadlet files for '{name}': {e}");
        }
        if let Err(e) = systemd::reset_failed(name) {
            eprintln!("Warning: failed to reset failed state for '{name}': {e}");
        }
    }

    // 3. Optionally delete the TOML definition
    if remove_config {
        if let Some(config_path) = podbox::config::find_config_path(name) {
            std::fs::remove_file(&config_path)?;
            println!("Config '{}' removed.", config_path.display());
        }
    }

    println!("Container '{name}' removed.");
    let _ = podbox::history::record(name, "remove", "container removed");

    // 4. Optionally remove the home directory
    if home {
        let home = &config.container.home;
        if home.exists() {
            if !force {
                let ctx = podbox::ui::Ctx::new(false, force, dry_run);
                if podbox::ui::confirm(
                    &ctx,
                    &format!("Remove home directory '{}'?", home.display()),
                    false,
                )
                .is_err()
                {
                    println!("Home directory kept.");
                    return Ok(());
                }
            }
            let status = std::process::Command::new("podman")
                .args(["unshare", "rm", "-rf"])
                .arg(home)
                .status()
                .context("failed to run podman unshare")?;
            if !status.success() {
                anyhow::bail!(
                    "Failed to delete home directory '{}' via podman unshare (sub-UID files need rootless namespace)",
                    home.display()
                );
            }
            println!("Home directory '{}' removed.", home.display());
        }
    }

    Ok(())
}

/// Find orphaned Quadlet files that have no matching TOML config.
///
/// A container is stale only when its `.container` Quadlet file exists on
/// disk but the corresponding `~/.config/podbox/{profiles/,}<name>.toml` has
/// been deleted. Stopped or failed containers with a config are never stale.
fn find_stale_containers() -> Vec<String> {
    let mut stale = Vec::new();

    for name in podbox::quadlet_install::list_installed_names() {
        if podbox::config::find_by_runtime_name(&name).is_none() {
            stale.push(name);
        }
    }

    stale
}

/// Remove orphaned Quadlet files (those whose TOML config has been deleted).
///
/// Only containers with no matching TOML config are considered stale.
/// Stopped or failed containers with an existing config are never touched.
pub fn run_remove_stale(dry_run: bool, force: bool) -> Result<()> {
    let stale = find_stale_containers();
    if stale.is_empty() {
        println!("No stale containers found.");
        return Ok(());
    }

    println!("Orphaned Quadlet runtimes found:");
    for name in &stale {
        println!("  {name}  (no config TOML)");
    }

    if !force {
        let ctx = podbox::ui::Ctx::new(false, force, dry_run);
        if let Err(e) = podbox::ui::confirm(&ctx, "Remove these?", false) {
            return Err(e.into());
        }
    }

    for name in &stale {
        if dry_run {
            println!("Would remove: {name}");
            continue;
        }

        if let Err(e) = podbox::quadlet_install::uninstall(name) {
            eprintln!("Warning: failed to uninstall '{name}': {e}");
        }

        // `--ignore`: the stale runtime may have no container left to remove.
        if let Err(e) = podbox::process::run_checked(
            "podman",
            &podbox::process::args(&["rm", "-f", "--ignore", name]),
        ) {
            eprintln!("Warning: failed to remove container '{name}': {e}");
        }

        if let Err(e) = systemd::reset_failed(name) {
            eprintln!("Warning: failed to reset failed state for '{name}': {e}");
        }

        println!("✓ Stale runtime files for '{name}' removed");
    }

    Ok(())
}
