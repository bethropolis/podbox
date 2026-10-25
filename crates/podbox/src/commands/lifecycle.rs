//! Container lifecycle commands: build, enable/disable, start/stop,
//! update, remove. Snapshot/restore live in [`snapshot`].

mod snapshot;

pub use snapshot::{run_restore, run_snapshot, run_snapshot_list, run_snapshot_prune};

use anyhow::{Context, Result};

use podbox::config::Config;
use podbox::env::HostEnv;
use podbox::systemd;
use podbox::xdg::ResolvedXdgDirs;

use super::runner::Runtime;

/// Build the container image (or pull a prebuilt image).
pub fn run_build(
    config: &Config,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
    dry_run: bool,
    rebuild: bool,
    no_diff: bool,
    output: podbox::cli::OutputFormat,
) -> Result<()> {
    podbox::quadlet_install::preflight::resolve_x11_dependencies(config)?;
    if rebuild && config.lifecycle.auto_checkpoint {
        checkpoint_previous(config, &config.container.name, dry_run)?;
    }
    podbox::build::run(config, env, xdg, dry_run, rebuild, output)?;
    if !dry_run {
        let _ = podbox::history::record(&config.container.name, "build", "");
        if config.lifecycle.quadlet && !matches!(output, podbox::cli::OutputFormat::Ndjson) {
            println!(
                "\nRun `podbox enable {}` to install Quadlet files.",
                config.container.name
            );
        }
    }
    // Post-build drift check (best-effort, human mode only: advisory text
    // has no place in an event stream).
    if !dry_run && !no_diff && !matches!(output, podbox::cli::OutputFormat::Ndjson) {
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
    yes: bool,
) -> Result<()> {
    // Guard: reinstalling Quadlet on a running container restarts its systemd unit.
    if !dry_run {
        let name = &config.container.name;
        let is_running = podbox::podman::query_state(name)
            .map(|s| s == podbox::podman::ContainerState::Running)
            .unwrap_or(false);
        if is_running {
            if !yes {
                let ctx = podbox::ui::Ctx::new(yes, false, dry_run);
                let prompt = format!(
                    "Container '{name}' is running — reinstalling Quadlet will reload systemd and may restart it. Continue?"
                );
                match podbox::ui::confirm(&ctx, &prompt, false) {
                    Ok(()) => {}
                    Err(podbox::error::PodboxError::ConfirmationRequired) => {
                        anyhow::bail!(
                            "Container '{name}' is running — refusing to reinstall Quadlet without --yes (non-interactive). Use `podbox enable {name} --yes` or `podbox stop {name}` first."
                        );
                    }
                    Err(_) => {
                        anyhow::bail!(
                            "Aborted. Container '{name}' is still running. Rerun with --yes or stop it first: `podbox enable {name} --yes`"
                        );
                    }
                }
            }
            podbox::ui::warn(&format!(
                "Reinstalling Quadlet for running container '{name}' — systemd will reload and may restart it..."
            ));
        }
    }

    podbox::quadlet_install::install(config, env, xdg, dry_run)?;
    if !dry_run {
        let _ = podbox::history::record(&config.container.name, "enable", "");
        println!("\nRun `podbox shell` to start and enter the container.");
    }
    Ok(())
}

/// Remove Quadlet files (disable systemd container lifecycle).
pub fn run_disable(name: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        podbox::ui::would(&format!("remove Quadlet units and files for '{name}'"));
        return Ok(());
    }
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

    let local_tag = config.local_image_tag();
    if !podbox::podman::image_exists(&local_tag).unwrap_or(false) {
        println!("Image not found, building first...");
        podbox::build::run(
            config,
            env,
            xdg,
            false,
            false,
            podbox::cli::OutputFormat::Text,
        )?;
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
    // Declared apps/bins export here, not at enable: discovery execs into
    // the container, so it needs a running box. Re-running on every start
    // also heals exports deleted from the host since last time.
    podbox::export::ensure_exports(config, name);
    let _ = podbox::history::record(name, "start", "");
    Ok(())
}

/// Stop the container.
///
/// The "systemctl --user stop" vs "podman stop" decision (and its dry-run
/// line) lives in [`runner::Runtime::stop`]: through systemd when Quadlet
/// manages the box so systemd tracks the state transition (preventing a
/// stale "unknown" in subsequent `systemctl is-active` checks), plain
/// podman otherwise.
pub fn run_stop(config: &Config, name: &str, dry_run: bool) -> Result<()> {
    let ctx = crate::ctx::Ctx { dry_run };
    Runtime::for_config(config).stop(&ctx, config)?;
    if !dry_run {
        let _ = podbox::history::record(name, "stop", "");
    }
    Ok(())
}

/// Restart the container.
///
/// Same mechanism as the restart inside `update`: through `systemctl --user
/// restart` (after `reset-failed`, so a failed unit actually restarts) when
/// Quadlet manages the box, plain `podman restart` otherwise.
pub fn run_restart(config: &Config, name: &str, dry_run: bool) -> Result<()> {
    let ctx = crate::ctx::Ctx { dry_run };
    Runtime::for_config(config).restart(&ctx, config)?;
    if !dry_run {
        let _ = podbox::history::record(name, "restart", "");
    }
    Ok(())
}

/// Stop every running managed container.
///
/// Enumerates TOML configs (the same source as `podbox list`) and stops
/// those actually running. A config that fails to load doesn't block the
/// rest: it falls back to stopping by unit/podman directly, with a warning.
/// Idle timeout already stops boxes routinely, so no confirmation prompt.
pub fn run_stop_all(dry_run: bool) -> Result<()> {
    // Scans every definition to find running boxes; parse warnings for the
    // boxes it will not touch are noise here too.
    podbox::config::set_parse_warnings_quiet(true);
    let mut stopped = 0u32;
    let mut failed = 0u32;
    for path in podbox::config::list_configs() {
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // Podman and systemd know the box by `container.name`, which can differ
        // from the file name. A config that fails to load falls back to the stem.
        let loaded = podbox::config::Config::load(&path);
        let name = match &loaded {
            Ok(cfg) => cfg.runtime_name().to_string(),
            Err(_) => stem.clone(),
        };
        if !matches!(
            podbox::podman::query_state(&name),
            Ok(podbox::podman::ContainerState::Running)
        ) {
            continue;
        }
        if dry_run {
            podbox::ui::would(&format!("stop {name}"));
            stopped += 1;
            continue;
        }
        let result = match loaded {
            Ok(cfg) => run_stop(&cfg, &name, false),
            Err(e) => {
                eprintln!(
                    "Warning: config for '{stem}' failed to load ({e:#}); stopping directly."
                );
                // The config is gone, so the quadlet decision comes from what
                // is actually installed rather than from a config flag.
                let ctx = crate::ctx::Ctx { dry_run: false };
                Runtime::for_installed(&stem).stop_named(&ctx, &stem)
            }
        };
        match result {
            Ok(()) => {
                println!("Stopped {name}");
                stopped += 1;
            }
            Err(e) => {
                eprintln!("Warning: failed to stop '{name}': {e:#}");
                failed += 1;
            }
        }
    }
    if stopped == 0 && failed == 0 {
        println!("No running containers.");
    }
    if failed > 0 {
        anyhow::bail!("{failed} container(s) failed to stop");
    }
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

fn checkpoint_previous(config: &Config, name: &str, dry_run: bool) -> Result<()> {
    let local_latest = config.local_image_tag();
    let fallback =
        if config.image.source().is_prebuilt() && config.image.packages.install.is_empty() {
            config.image.image_ref.as_deref().unwrap_or(&local_latest)
        } else {
            &local_latest
        };
    let previous = format!("{}:checkpoint-prev", config.image_repo());
    if dry_run {
        println!("podman tag \"$(podman inspect --format '{{{{.Image}}}}' {name})\" {previous}");
        return Ok(());
    }
    let inspect = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["inspect", "--format", "{{.Image}}", name]),
    );
    let current = inspect
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|image| !image.is_empty())
        .or_else(|| {
            podbox::podman::image_exists(fallback)
                .ok()
                .filter(|exists| *exists)
                .map(|_| fallback.to_string())
        });
    let Some(current) = current else {
        return Ok(());
    };
    let output = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["tag", &current, &previous]),
    )?;
    if !output.status.success() {
        anyhow::bail!("failed to checkpoint current image '{current}'");
    }
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
    if all {
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
