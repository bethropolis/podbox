//! Image build and Quadlet lifecycle installation.

use anyhow::Result;

use podbox::config::Config;
use podbox::env::HostEnv;
use podbox::xdg::ResolvedXdgDirs;

use super::checkpoint::checkpoint_previous;

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

    podbox::quadlet_install::install_with_ctx(
        config,
        env,
        xdg,
        &podbox::ui::Ctx::new(yes, false, dry_run),
    )?;
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
