//! Pre-install validation: mount paths, port conflicts, and the admin
//! capability-preset confirmation.
//!
//! Extracted verbatim from `quadlet_install.rs`.

use anyhow::{Context, Result};

use crate::config::Config;

/// Validate that mount paths referenced in extra mounts exist on the host.
pub(crate) fn preflight_check(config: &Config) -> Result<()> {
    let name = &config.container.name;

    // Check home directory
    if !config.container.home.exists() {
        eprintln!(
            "  Note: home directory '{}' will be created (does not exist yet).",
            config.container.home.display()
        );
    }

    // Parse extra mounts and check host paths
    for mount in &config.container.mounts.extra {
        let host_path = match mount.split_once(':') {
            Some((host, _)) => host,
            None => mount,
        };
        let path = std::path::Path::new(host_path);
        if !path.exists() {
            if crate::codegen::distros::is_tty() {
                let prompt = format!(
                    "Mount path '{}' does not exist on the host. Create it?",
                    path.display()
                );
                let create =
                    dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                        .with_prompt(prompt)
                        .default(true)
                        .interact_opt()?;
                if create == Some(true) {
                    std::fs::create_dir_all(path).with_context(|| {
                        format!("failed to create mount directory '{}'", path.display())
                    })?;
                    println!("✓ Directory '{}' created.", path.display());
                } else {
                    eprintln!(
                        "Warning: mount path '{}' does not exist on the host. This may cause the container to fail to load.",
                        path.display()
                    );
                }
            } else {
                eprintln!(
                    "Warning: mount path '{}' does not exist on the host (container '{}').",
                    path.display(),
                    name
                );
            }
        }
    }

    // Intelligently check if container is running
    let is_running = crate::podman::query_state(name)
        .map(|state| state == crate::podman::ContainerState::Running)
        .unwrap_or(false);

    // Only run port bind tests if the container is stopped
    if is_running {
        println!(
            "  Note: container '{name}' is running. Skipping port conflict checks for upgrade."
        );
        return Ok(());
    }

    // Check for port conflicts (IPv4 + IPv6, TCP + UDP)
    let conflicts = crate::ports::check_host_ports(&config.network.ports);
    if !conflicts.is_empty() {
        let listed = conflicts
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::bail!(
            "Port conflict: already in use on the host — {listed}. \
             Find the process with: `ss -ltnp 'sport = :<port>'`"
        );
    }

    // Check admin cap_preset
    if config.security.cap_preset == crate::config::CapPreset::Admin {
        if crate::codegen::distros::is_tty() {
            let caps = config.security.cap_preset.caps().join(", ");
            let confirmed = dialoguer::Confirm::with_theme(
                &dialoguer::theme::ColorfulTheme::default(),
            )
            .with_prompt(format!(
                "WARNING: CapPreset::Admin grants {caps}. Only proceed if you fully trust this container. Continue?"
            ))
            .default(false)
            .interact()?;
            if !confirmed {
                anyhow::bail!(
                    "Aborted — set cap_preset to a lower level or use cap_add for specific caps"
                );
            }
        } else {
            let caps = config.security.cap_preset.caps().join(", ");
            eprintln!(
                "Note: cap_preset = \"admin\" grants {caps}. Non-interactive mode, continuing without confirmation."
            );
        }
    }

    Ok(())
}
