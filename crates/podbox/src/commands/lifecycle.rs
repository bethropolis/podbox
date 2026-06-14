use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};

use podbox::config::Config;
use podbox::env::HostEnv;
use podbox::systemd;
use podbox::xdg::ResolvedXdgDirs;

fn snapshot_tag(tag: &str, name: &str) -> String {
    format!("localhost/podbox-{}:snapshot-{}", name, tag)
}

fn snapshots_dir() -> PathBuf {
    podbox::config::config_dir().join("snapshots")
}

/// Snapshot the current container state as a tagged image.
pub fn run_snapshot(_config: &Config, name: &str, tag: Option<&str>) -> Result<()> {
    let tag: String = match tag {
        Some(t) => t.to_string(),
        None => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string()),
    };

    let container_name = format!("podbox-{}", name);
    let image_tag = snapshot_tag(&tag, name);

    eprintln!(
        "Snapshotting container '{}' as '{}'...",
        container_name, image_tag
    );

    let output = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["commit", &container_name, &image_tag]),
    )?;
    print!("{}", String::from_utf8_lossy(&output.stdout));

    // Store metadata
    let dir = snapshots_dir().join(name);
    std::fs::create_dir_all(&dir)?;
    let meta_path = dir.join(format!("{}.toml", tag));
    let now_rfc = date_now_rfc3339();
    let meta = format!(
        "tag = \"{}\"\ncreated = \"{}\"\nimage = \"{}\"\n",
        tag, now_rfc, image_tag
    );
    std::fs::write(&meta_path, &meta)?;

    println!("✓ Snapshot '{}' saved (tag: {})", image_tag, tag);
    Ok(())
}

fn date_now_rfc3339() -> String {
    // Simple RFC 3339 without chrono
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Days since epoch
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Compute year/month/day from days since epoch
    let (year, month, day) = days_to_date(days as i64);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_date(days: i64) -> (i64, u32, u32) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

/// Restore a container from a snapshot image.
pub fn run_restore(_config: &Config, name: &str, tag: &str) -> Result<()> {
    let snapshot_img = snapshot_tag(tag, name);
    let latest_img = format!("localhost/podbox-{}:latest", name);

    // Verify snapshot exists
    let exists = podbox::podman::image_exists(&snapshot_img).unwrap_or(false);
    if !exists {
        anyhow::bail!("Snapshot '{}' not found as image '{}'", tag, snapshot_img);
    }

    // Stop the container
    eprintln!("Stopping container 'podbox-{}'...", name);
    let _ = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["stop", &format!("podbox-{}", name)]),
    );

    // Re-tag snapshot as the main image
    eprintln!("Restoring from snapshot '{}'...", snapshot_img);
    let output = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["tag", &snapshot_img, &latest_img]),
    )?;
    if !output.status.success() {
        anyhow::bail!("Failed to tag snapshot image");
    }

    // Start the container
    eprintln!("Starting container...");
    let _ = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["start", &format!("podbox-{}", name)]),
    );

    println!("✓ Restored '{}' from snapshot '{}'", name, tag);
    Ok(())
}

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
    if !dry_run && config.lifecycle.quadlet {
        println!("\nRun `podbox enable` to install Quadlet files.");
    }
    // Post-build drift check (best-effort).
    if !dry_run && !no_diff {
        let name = &config.container.name;
        if let Ok(state) = podbox::podman::query_state(name) {
            if state == podbox::podman::ContainerState::Running {
                match podbox::diff::compute(config, name, &env.username) {
                    Ok(result) if result.has_drift => {
                        println!("\n── Package drift detected ──");
                        println!("{}", podbox::diff::format_report(&result));
                        println!("Run `podbox diff --apply` to update the TOML.");
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("Warning: drift check skipped ({})", e),
                }
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
        println!("\nRun `podbox shell` to start and enter the container.");
    }
    Ok(())
}

/// Remove Quadlet files (disable systemd container lifecycle).
pub fn run_disable(name: &str) -> Result<()> {
    podbox::quadlet_install::uninstall(name)
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
        println!("podman start {}", name);
        return Ok(());
    }

    let local_tag = format!("localhost/podbox-{}:latest", config.image.name);
    if !podbox::podman::image_exists(&local_tag).unwrap_or(false) {
        println!("Image not found, building first...");
        podbox::build::run(config, env, xdg, false, false)?;
    }

    let qdir = dirs::config_dir()
        .unwrap_or_else(|| podbox::config::expand_tilde("~/.config"))
        .join("containers/systemd");
    let container_file = qdir.join(format!("{}.container", name));
    if !container_file.exists() {
        println!("Quadlet files not found, installing...");
        podbox::quadlet_install::install(config, env, xdg, false)?;
    }

    println!("Starting container...");
    crate::commands::ensure_running(name, false, timeout_secs)?;
    println!("Container '{}' is running!", name);
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
            println!("systemctl --user stop {}", name);
        } else {
            println!("podman stop {}", name);
        }
        return Ok(());
    }
    if config.lifecycle.quadlet && systemd::is_available() {
        systemd::stop_unit(name)
    } else {
        let args = podbox::process::args(&["stop", name]);
        podbox::process::spawn_interactive("podman", &args).map(|_| ())
    }
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
        println!("podbox update: pull/rebuild and restart {}", name);
        println!("  build::run(config, env, xdg, dry_run: true, rebuild: true)");
        if !no_restart {
            if config.lifecycle.quadlet && systemd::is_available() {
                println!("  systemctl --user restart {}", name);
            } else {
                println!("  podman restart {}", name);
            }
        }
        return Ok(());
    }

    println!("Updating '{}'...", name);

    podbox::build::run(config, env, xdg, false, true)?;

    if no_restart {
        println!("Image updated. Restart skipped (--no-restart).");
        return Ok(());
    }

    println!("Restarting container...");
    if config.lifecycle.quadlet && systemd::is_available() {
        systemd::restart_unit(name)?;
    } else {
        let args = podbox::process::args(&["restart", name]);
        podbox::process::spawn_interactive("podman", &args)?;
    }

    println!("Update complete.");
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
        println!("podman stop {}", name);
        println!("podman rm -f {}", name);
        if config.lifecycle.quadlet {
            println!("quadlet_install::uninstall({})", name);
            println!("systemctl --user reset-failed {}.service", name);
        }
        if remove_config {
            println!("rm {}.toml", podbox::config::config_dir().join(name).display());
        }
        if all {
            println!("rm -rf {}", config.container.home.display());
        }
        return Ok(());
    }

    if !force {
        print!("Remove container '{}'? [y/N] ", name);
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // 1. Stop and remove the podman container (best-effort)
    let _ = podbox::process::run_piped("podman", &podbox::process::args(&["stop", name]));
    let _ = podbox::process::run_piped("podman", &podbox::process::args(&["rm", "-f", name]));

    // 2. Clean up Quadlet files and systemd units
    if config.lifecycle.quadlet {
        let _ = systemd::stop_unit(name);
        let _ = podbox::quadlet_install::uninstall(name);
        let _ = systemd::reset_failed(name);
    }

    // 3. Optionally delete the TOML definition
    if remove_config {
        let config_path = podbox::config::config_dir().join(format!("{}.toml", name));
        if config_path.exists() {
            std::fs::remove_file(&config_path)?;
            println!("Config '{}' removed.", config_path.display());
        }
    }

    println!("Container '{}' removed.", name);

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
/// disk but the corresponding `~/.config/podbox/<name>.toml` has been
/// deleted.  Stopped or failed containers with a config are never stale.
fn find_stale_containers() -> Vec<String> {
    let qdir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.config"))
        .join("containers/systemd");
    let config_dir = podbox::config::config_dir();

    let mut stale = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&qdir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "container").unwrap_or(false) {
                let name = path.file_stem().unwrap().to_string_lossy().to_string();
                let config_path = config_dir.join(format!("{}.toml", name));
                if !config_path.exists() {
                    stale.push(name);
                }
            }
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
        println!("  {}  (no config TOML)", name);
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
            println!("Would remove: {}", name);
            continue;
        }

        if let Err(e) = podbox::quadlet_install::uninstall(name) {
            eprintln!("Warning: failed to uninstall '{}': {}", name, e);
        }

        let _ = podbox::process::run_piped("podman", &podbox::process::args(&["rm", "-f", name]));

        let _ = systemd::reset_failed(name);

        println!("✓ Stale runtime files for '{}' removed", name);
    }

    Ok(())
}
