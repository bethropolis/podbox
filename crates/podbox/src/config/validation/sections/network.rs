use crate::config::{Config, NetworkMode};

pub fn validate_network(config: &Config, errors: &mut Vec<String>) {
    // `network.mode` is a `NetworkMode` enum: unknown spellings fail at
    // parse time with the valid list.
    if config.network.effective_mode() == NetworkMode::None && !config.network.ports.is_empty() {
        errors.push("network.ports: cannot publish ports when network mode is none/offline".into());
    }

    if config.integration.x11 && !config.integration.wayland {
        errors
            .push("integration.x11: x11 needs wayland (the satellite is a Wayland client)".into());
    }
    if config.integration.x11 && config.network.effective_mode() == NetworkMode::Host {
        errors.push(
            "integration.x11: x11 cannot use network.mode = \"host\" because DISPLAY=:0 could reach the host X server's abstract socket".into(),
        );
    }

    for (i, port) in config.network.ports.iter().enumerate() {
        if !port.contains(':') {
            errors.push(format!(
                "network.ports[{i}]: '{port}' is invalid (expected 'hostPort:containerPort' or 'ip:hostPort:containerPort')"
            ));
        }
    }
}
