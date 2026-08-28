//! Container lifecycle commands: build, enable/disable, start/stop,
//! update, remove. Snapshot/restore live in [`snapshot`].

mod snapshot;

pub use snapshot::{run_restore, run_snapshot, run_snapshot_list, run_snapshot_prune};

use std::io::Write;

use anyhow::{Context, Result};

use podbox::config::Config;
use podbox::env::HostEnv;
use podbox::systemd;
use podbox::xdg::ResolvedXdgDirs;

/// Build the container image (or pull a prebuilt image).
pub fn run_build(
    config: &Config,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
    dry_run: bool,
    rebuild: bool,
    no_diff: bool,
) -> Result<()> {
    podbox::build::run(config, env, xdg, dry_run, rebuild)?;
    if !dry_run {
        let _ = podbox::history::record(&config.container.name, "build", "");
        if config.lifecycle.quadlet {
            println!("\nRun `podbox enable` to install Quadlet files.");
        }
    }
    // Post-build drift check (best-effort).
    if !dry_run && !no_diff {
        let name = &config.container.name;
        if let Ok(state) = podbox::podman::query_state(name)
            && state == podbox::podman::ContainerState::Running
        {
            match podbox::diff::compute(config, name, &env.username) {
                Ok(result) if result.has_drift => {
                    println!("\n── Package drift detected ──");
                    println!("{}", podbox::diff::format_report(&result));
                    println!("Run `podbox diff --apply` to update the TOML.");
                }
                Ok(_) => {}
                Err(e) => eprintln!("Warning: drift check skipped ({e})"),
            }
        }
    }
    Ok(())
}

/// Install Quadlet files (enable systemd container lifecycle).
pub fn run_enable(
    config: &Config,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
    dry_run: bool,
) -> Result<()> {
    podbox::quadlet_install::install(config, env, xdg, dry_run)?;
    if !dry_run {
        let _ = podbox::history::record(&config.container.name, "enable", "");
        println!("\nRun `podbox shell` to start and enter the container.");
    }
    Ok(())
}

/// Remove Quadlet files (disable systemd container lifecycle).
pub fn run_disable(name: &str) -> Result<()> {
    podbox::quadlet_install::uninstall(name)?;
    let _ = podbox::history::record(name, "disable", "");
    Ok(())
}

/// Start the container, auto-healing missing images and Quadlet files.
pub fn run_start(
    config: &Config,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
    name: &str,
    dry_run: bool,
    timeout_secs: u64,
) -> Result<()> {
    if dry_run {
        println!("podman start {name}");
        return Ok(());
    }

    let local_tag = format!("localhost/podbox-{}:latest", config.image.name);
    if !podbox::podman::image_exists(&local_tag).unwrap_or(false) {
        println!("Image not found, building first...");
        podbox::build::run(config, env, xdg, false, false)?;
    }

    if !podbox::quadlet_install::is_installed(name) {
        println!("Quadlet files not found, installing...");
        podbox::quadlet_install::install(config, env, xdg, false)?;
    }

    // Abort early with a clear, actionable message if a published host port
    // is already occupied — pasta would otherwise fail and the start would
    // surface as a cryptic systemd unit failure. Skipped when the container
    // is already running (pasta itself then holds the port).
    let already_running = podbox::podman::query_state(name)
        .map(|s| s == podbox::podman::ContainerState::Running)
        .unwrap_or(false);
    if !already_running {
        let conflicts = podbox::ports::check_host_ports(&config.network.ports);
        if !conflicts.is_empty() {
            let mut msg = String::from("Cannot start: published host port(s) already in use:\n");
            for c in &conflicts {
                use std::fmt::Write as _;
                let _ = writeln!(msg, "  - {c}");
            }
            msg.push_str("\nFind the process with: `ss -ltnp 'sport = :<port>'`\n");
            msg.push_str("Either stop that process or change the mapping in [network]ports.");
            anyhow::bail!(msg);
        }
    }

    println!("Starting container...");
    crate::commands::ensure_running(name, false, timeout_secs)?;
    println!("Container '{name}' is running!");
    let _ = podbox::history::record(name, "start", "");
    Ok(())
}

/// Stop the container.
///
/// Uses `systemctl --user stop` when quadlet is enabled so that systemd
/// tracks the service state transition (preventing a stale "unknown" in
/// subsequent `systemctl is-active` checks).
pub fn run_stop(config: &Config, name: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        if config.lifecycle.quadlet && systemd::is_available() {
            println!("systemctl --user stop {name}");
        } else {
            println!("podman stop {name}");
        }
        return Ok(());
    }
    if config.lifecycle.quadlet && systemd::is_available() {
        systemd::stop_unit(name)?;
    } else {
        let args = podbox::process::args(&["stop", name]);
        podbox::process::spawn_interactive("podman", &args)?;
    }
    let _ = podbox::history::record(name, "stop", "");
    Ok(())
}

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
        println!("podbox update: pull/rebuild and restart {name}");
        println!("  build::run(config, env, xdg, dry_run: true, rebuild: true)");
        if !no_restart {
            if config.lifecycle.quadlet && systemd::is_available() {
                println!("  systemctl --user restart {name}");
            } else {
                println!("  podman restart {name}");
            }
        }
        return Ok(());
    }

    println!("Updating '{name}'...");

    podbox::build::run(config, env, xdg, false, true)?;

    if no_restart {
        println!("Image updated. Restart skipped (--no-restart).");
        return Ok(());
    }

    println!("Restarting container...");
    if config.lifecycle.quadlet && systemd::is_available() {
        systemd::reset_failed(name)?;
        systemd::restart_unit(name)?;
    } else {
        let args = podbox::process::args(&["restart", name]);
        podbox::process::spawn_interactive("podman", &args)?;
    }

    println!("Update complete.");
    let _ = podbox::history::record(name, "update", "");
    Ok(())
}

/// Remove a container and optionally its home directory.
pub fn run_remove(
    config: &Config,
    name: &str,
    dry_run: bool,
    all: bool,
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
        if all {
            println!("rm -rf {}", config.container.home.display());
        }
        return Ok(());
    }

    if !force {
        print!("Remove container '{name}'? [y/N] ");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // 1. Stop and remove the podman container (best-effort)
    if let Err(e) = podbox::process::run_piped("podman", &podbox::process::args(&["stop", name])) {
        eprintln!("Warning: failed to stop container '{name}': {e}");
    }
    if let Err(e) =
        podbox::process::run_piped("podman", &podbox::process::args(&["rm", "-f", name]))
    {
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
    if all {
        let home = &config.container.home;
        if home.exists() {
            if !force {
                print!("Remove home directory '{}'? [y/N] ", home.display());
                std::io::stdout().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
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
        if podbox::config::find_config_path(&name).is_none() {
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
        print!("Remove these? [y/N] ");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
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

        if let Err(e) =
            podbox::process::run_piped("podman", &podbox::process::args(&["rm", "-f", name]))
        {
            eprintln!("Warning: failed to remove container '{name}': {e}");
        }

        if let Err(e) = systemd::reset_failed(name) {
            eprintln!("Warning: failed to reset failed state for '{name}': {e}");
        }

        println!("✓ Stale runtime files for '{name}' removed");
    }

    Ok(())
}

