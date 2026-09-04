//! Host-exposure summary for `podbox doctor`.
//!
//! Extracted verbatim from `doctor.rs`; see `super` for the check surface.

use owo_colors::{OwoColorize, Stream};

use podbox::config::Config;

/// Plain-language summary of what this container can reach on the host.
/// Printed after the checks so exposure can be audited at a glance.
/// Container-specific rows (home, network, mounts) are at the bottom.
pub(crate) fn print_exposure_summary(config: &Config) {
    let on = |b: bool| if b { "enabled" } else { "off" }.to_string();
    let active = podbox::config::read_active_context();
    let is_active = active.as_deref() == Some(config.container.name.as_str());
    let title = if is_active {
        format!(
            "Host exposure — container '{}' (active)",
            config.container.name
        )
    } else {
        format!("Host exposure — container '{}'", config.container.name)
    };
    println!(
        "\n{}",
        title.if_supports_color(Stream::Stdout, |s| s.bold())
    );
    let line = |k: &str, v: String| println!("  {k:<22} {v}");

    // Integration / host-level first
    line("Wayland (GUI)", on(config.integration.wayland));
    line("Audio (PipeWire)", on(config.integration.audio));
    line("GPU", format!("{:?}", config.integration.gpu));
    line(
        "D-Bus",
        if config.integration.dbus {
            format!("proxied; talk list: {}", {
                let talk = config.dbus_effective_talk();
                if talk.is_empty() {
                    "none".to_string()
                } else {
                    talk.join(", ")
                }
            })
        } else {
            "off".to_string()
        },
    );
    line("Clipboard", on(config.integration.clipboard));
    line("Notifications", on(config.integration.notify));
    line("URL opening (xdg-open)", on(config.integration.xdg_open));
    line("SSH agent socket", on(config.integration.ssh_agent));
    line("GPG agent socket", on(config.integration.gpg_agent));
    match (
        &config.integration.host_exec.enabled,
        &config.integration.host_exec.allowlist,
    ) {
        (true, Some(list)) if !list.is_empty() => {
            line("Host exec", "ENABLED".to_string());
            let mut entries: Vec<_> = list.iter().collect();
            entries.sort_by_key(|(k, _)| *k);
            for (alias, entry) in entries {
                let shim = if entry.shim_enabled() { "yes" } else { "no" };
                let filter = if entry.filter_enabled() {
                    "ON (sanitized)"
                } else {
                    "OFF (unfiltered)"
                };
                // Keep alignment pleasant without over-formatting: bullet line
                println!(
                    "    • {:<12} → {:<28} [shim: {}, filter: {}]",
                    alias,
                    entry.path(),
                    shim,
                    filter
                );
            }
        }
        (true, Some(_)) => {
            line("Host exec", "ENABLED - allowlist empty".to_string());
        }
        (true, None) => {
            line("Host exec", "ENABLED - no allowlist?".to_string());
        }
        (false, _) => {
            line("Host exec", "off".to_string());
        }
    }
    // Hardware presets
    {
        let hw = &config.integration.hardware;
        let enabled: Vec<&str> = [
            hw.joystick.then_some("joystick"),
            hw.webcam.then_some("webcam"),
            hw.yubikey.then_some("yubikey"),
            hw.serial.then_some("serial"),
            hw.kvm.then_some("kvm"),
        ]
        .into_iter()
        .flatten()
        .collect();
        if enabled.is_empty() {
            line("Hardware presets", "none".to_string());
        } else {
            line("Hardware presets", enabled.join(", "));
        }
    }
    // Secrets
    if config.security.secrets.is_empty() {
        line("Secrets", "none".to_string());
    } else {
        line(
            "Secrets",
            format!("{} declared", config.security.secrets.len()),
        );
        for secret in &config.security.secrets {
            match secret {
                podbox::config::SecretEntry::Simple(name) => {
                    println!("    • {name} (env → {name}, podman)");
                }
                podbox::config::SecretEntry::Detailed {
                    name,
                    secret_type,
                    target,
                    mode,
                    source,
                } => {
                    let src = match source {
                        podbox::config::SecretSource::Podman => "podman",
                        podbox::config::SecretSource::Systemd => "systemd",
                    };
                    let tgt = target.as_deref().unwrap_or(name);
                    let extra = match secret_type {
                        podbox::config::SecretType::Mount => {
                            format!(", mode {}", mode.as_deref().unwrap_or("0400"))
                        }
                        _ => String::new(),
                    };
                    println!("    • {name} ({secret_type:?} → {tgt}{extra}, {src})");
                }
            }
        }
    }
    // Container-specific at the bottom
    line(
        "Home directory",
        format!(
            "{} (persistent container storage)",
            config.container.home.display()
        ),
    );
    line(
        "Network",
        if config.network.mode == "host" {
            "host mode - container shares the host's network stack".to_string()
        } else {
            format!(
                "{} ({})",
                config.network.mode,
                if config.network.ports.is_empty() {
                    "no published ports".to_string()
                } else {
                    format!("published ports: {}", config.network.ports.join(", "))
                }
            )
        },
    );
    line(
        "Extra mounts",
        if config.container.mounts.extra.is_empty() {
            "none".to_string()
        } else {
            config.container.mounts.extra.join(", ")
        },
    );
}
