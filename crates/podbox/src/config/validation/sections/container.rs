use crate::config::{Config, ServiceConfig};

use super::super::values::{MAX_CPUS, is_valid_env_key, is_valid_memory, validate_container_name};

pub fn validate_container(config: &Config, errors: &mut Vec<String>) {
    if config.container.name.trim().is_empty() {
        errors.push("container.name: must not be empty".into());
    } else if let Err(e) = validate_container_name("container", &config.container.name) {
        errors.push(format!("container.name: {e}"));
    }
    if config.container.home.as_os_str().is_empty() {
        errors.push("container.home: must not be empty".into());
    }
    if config.container.shell.trim().is_empty() {
        errors.push("container.shell: must not be empty".into());
    }
    if config.container.slice.trim().is_empty() || config.container.slice.contains('\n') {
        errors.push("container.slice: must be a non-empty systemd slice name".into());
    }
    if !(1..=10_000).contains(&config.container.cpu_weight) {
        errors.push("container.cpu_weight: must be between 1 and 10000".into());
    }
    if let Some(dotfiles) = &config.dotfiles {
        if dotfiles.source.trim().is_empty() {
            errors.push("dotfiles.source: must not be empty".into());
        } else if dotfiles.source == "host:" {
            errors.push("dotfiles.source: host: must be followed by a directory path".into());
        }
        if dotfiles.target.trim().is_empty() {
            errors.push("dotfiles.target: must not be empty".into());
        } else {
            let target = if dotfiles.target == "~" {
                ""
            } else {
                dotfiles
                    .target
                    .strip_prefix("~/")
                    .unwrap_or(&dotfiles.target)
            };
            let path = std::path::Path::new(target);
            if path.is_absolute()
                || (target.starts_with('~')
                    && dotfiles.target != "~"
                    && !dotfiles.target.starts_with("~/"))
                || path.components().any(|part| {
                    matches!(
                        part,
                        std::path::Component::ParentDir | std::path::Component::RootDir
                    )
                })
            {
                errors.push(
                    "dotfiles.target: must be a path inside the container home (for example '~/.dotfiles')".into(),
                );
            }
        }
        if dotfiles
            .install
            .as_ref()
            .is_some_and(|cmd| cmd.trim().is_empty())
        {
            errors.push("dotfiles.install: must not be empty when set".into());
        }
    }
    if let Some(ref mem) = config.container.memory {
        if !is_valid_memory(mem) {
            errors.push(format!(
                "container.memory: '{mem}' is not a valid memory limit (e.g. '2g', '512m')"
            ));
        }
    }
    if let Some(ref cpus) = config.container.cpus {
        // `NaN`/`inf` parse as f64 but are meaningless as CPU counts;
        // without the finiteness check they sailed through to
        // `PodmanArgs=--cpus=NaN`.
        let valid = cpus
            .parse::<f64>()
            .is_ok_and(|v| v.is_finite() && v > 0.0 && v <= MAX_CPUS);
        if !valid {
            errors.push(format!(
                "container.cpus: '{cpus}' is not a valid CPU count (e.g. '2.0', '0.5')"
            ));
        }
    }
    for (i, mount) in config.container.mounts.extra.iter().enumerate() {
        if !mount.contains(':') {
            errors.push(format!(
                "container.mounts.extra[{i}]: '{mount}' missing ':' separator (expected host:container[:options])"
            ));
        }
    }
    for (key, val) in &config.container.env.values {
        if !is_valid_env_key(key) {
            errors.push(format!(
                "container.env: invalid key '{key}' (use ^[A-Za-z_][A-Za-z0-9_]*$)"
            ));
        }
        if key.contains('\n') {
            errors.push(format!("container.env: key {key:?} contains newline"));
        }
        if val.contains('\n') {
            errors.push(format!("container.env: value for {key:?} contains newline"));
        }
    }
}

pub fn validate_env_forward_and_services(config: &Config, errors: &mut Vec<String>) {
    for (i, pattern) in config.container.env.forward.iter().enumerate() {
        // A trailing `*` marks a prefix forward (`FOO_*`); the key
        // itself must still be a valid env name.
        let key = pattern.strip_suffix('*').unwrap_or(pattern);
        if !is_valid_env_key(key) {
            errors.push(format!(
                "container.env.forward[{i}]: '{pattern}' must be an environment key or PREFIX_*"
            ));
        }
    }
    for (name, service) in &config.container.services {
        let valid_name = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid_name {
            errors.push(format!("container.services: invalid service name '{name}'"));
        }
        let (command, env) = match service {
            ServiceConfig::Short(command) => (command, None),
            ServiceConfig::Detailed(detail) => (&detail.command, Some(&detail.env)),
        };
        if command.trim().is_empty() {
            errors.push(format!(
                "container.services.{name}.command: must not be empty"
            ));
        }
        // `restart` is a `ServiceRestart` enum: unknown spellings fail
        // at parse time with the valid list, so there is nothing to
        // re-check here.
        if let Some(values) = env {
            for (key, value) in values {
                if !is_valid_env_key(key) || value.chars().any(|c| matches!(c, '\0' | '\n' | '\r'))
                {
                    errors.push(format!(
                        "container.services.{name}.env: invalid environment entry '{key}'"
                    ));
                }
            }
        }
    }
}
