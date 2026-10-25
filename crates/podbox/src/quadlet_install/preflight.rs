//! Pre-install validation: mount paths, port conflicts, and the admin
//! capability-preset confirmation.
//!
//! Extracted verbatim from `quadlet_install.rs`.

use anyhow::{Context, Result};

use crate::config::Config;

pub struct X11Dependencies {
    pub satellite: std::path::PathBuf,
    pub xwayland: std::path::PathBuf,
}

pub fn resolve_x11_dependencies(config: &Config) -> Result<Option<X11Dependencies>> {
    if !config.integration.x11 {
        return Ok(None);
    }

    let satellite = which::which("xwayland-satellite").map_err(|_| {
        anyhow::anyhow!(
            "x11 requires xwayland-satellite on the host. Install it with: Arch: pacman -S xwayland-satellite; Fedora: dnf install xwayland-satellite; Alpine edge: apk add xwayland-satellite; openSUSE: zypper in xwayland-satellite; Debian/Ubuntu: cargo install xwayland-satellite or use the upstream release binary"
        )
    })?;
    let xwayland = which::which("Xwayland").map_err(|_| {
        anyhow::anyhow!(
            "x11 requires Xwayland on the host. Install the Xwayland package for your distribution and ensure Xwayland is on PATH"
        )
    })?;

    Ok(Some(X11Dependencies {
        satellite,
        xwayland,
    }))
}

/// Validate that mount paths referenced in extra mounts exist on the host.
pub(crate) fn preflight_check(config: &Config, ctx: &crate::ui::Ctx) -> Result<()> {
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
            let prompt = format!(
                "Mount path '{}' does not exist on the host. Create it?",
                path.display()
            );
            match crate::ui::confirm(ctx, &prompt, true) {
                Ok(()) => {
                    std::fs::create_dir_all(path).with_context(|| {
                        format!("failed to create mount directory '{}'", path.display())
                    })?;
                    println!("✓ Directory '{}' created.", path.display());
                }
                // Without a terminal there is nobody to ask — warn and let the
                // install fail downstream with its own error, like before.
                Err(crate::error::PodboxError::ConfirmationRequired) => eprintln!(
                    "Warning: mount path '{}' does not exist on the host (container '{}').",
                    path.display(),
                    name
                ),
                Err(_) => eprintln!(
                    "Warning: mount path '{}' does not exist on the host. This may cause the container to fail to load.",
                    path.display()
                ),
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

    // Check declared secrets exist in podman store
    check_declared_secrets(config)?;

    // Check admin cap_preset
    if config.security.cap_preset == crate::config::CapPreset::Admin {
        let caps = config.security.cap_preset.caps().join(", ");
        let prompt = format!(
            "WARNING: CapPreset::Admin grants {caps}. Only proceed if you fully trust this container. Continue?"
        );
        match crate::ui::confirm(ctx, &prompt, false) {
            Ok(()) => {}
            // Same trade the old non-TTY branch made: an unattended install
            // of an admin-preset config is documented, not silent.
            Err(crate::error::PodboxError::ConfirmationRequired) => eprintln!(
                "Note: cap_preset = \"admin\" grants {caps}. Non-interactive mode, continuing without confirmation."
            ),
            Err(crate::error::PodboxError::Cancelled) => anyhow::bail!(
                "Aborted — set cap_preset to a lower level or use cap_add for specific caps"
            ),
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

pub(crate) fn check_declared_secrets(config: &Config) -> Result<()> {
    use std::collections::HashSet;

    use crate::config::{SecretEntry, SecretSource};

    if config.security.secrets.is_empty() {
        return Ok(());
    }

    let output = std::process::Command::new("podman")
        .args(["secret", "ls", "--format", "{{.Name}}"])
        .output();

    let available: HashSet<String> = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        Ok(o) => {
            // podman secret ls failed — treat as empty and warn
            eprintln!(
                "Warning: `podman secret ls` failed ({}): {}",
                o.status,
                String::from_utf8_lossy(&o.stderr).trim()
            );
            HashSet::new()
        }
        Err(e) => {
            eprintln!("Warning: failed to run `podman secret ls`: {e}");
            HashSet::new()
        }
    };

    for secret in &config.security.secrets {
        let (name, source) = match secret {
            SecretEntry::Simple(n) => (n.as_str(), SecretSource::Podman),
            SecretEntry::Detailed { name, source, .. } => (name.as_str(), *source),
        };
        if source == SecretSource::Podman && !available.contains(name) {
            anyhow::bail!(
                "Required secret '{name}' not found in Podman secret store.\n\
                 Create it with: `podman secret create {name} -`"
            );
        }
    }
    Ok(())
}
