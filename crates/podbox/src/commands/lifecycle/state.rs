//! Container start, stop, and restart operations.

use anyhow::Result;

use podbox::config::Config;
use podbox::env::HostEnv;
use podbox::xdg::ResolvedXdgDirs;

use crate::commands::runner::Runtime;

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
/// line) lives in [`crate::commands::runner::Runtime::stop`]: through systemd when Quadlet
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
