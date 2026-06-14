use anyhow::Result;

use crate::config::Config;
use crate::error::PodboxError;

impl Config {
    pub fn validate(&self) -> Result<()> {
        let mut errors: Vec<String> = Vec::new();

        if self.image.base.trim().is_empty() {
            errors.push("image.base: must not be empty".into());
        }
        if self.image.name.trim().is_empty() {
            errors.push("image.name: must not be empty".into());
        } else if !is_valid_name(&self.image.name) {
            errors.push(format!(
                "image.name: '{}' contains invalid characters (use letters, digits, hyphens, underscores, dots)",
                self.image.name
            ));
        }
        if let Some(ref r) = self.image.image_ref {
            if r.trim().is_empty() {
                errors.push("image.image: must not be empty when set".into());
            } else if !r.contains(':') && !r.contains('/') {
                errors.push(format!(
                    "image.image: '{}' does not look like a valid image reference (missing ':' or '/')",
                    r
                ));
            }
        }

        if self.container.name.trim().is_empty() {
            errors.push("container.name: must not be empty".into());
        } else if !is_valid_name(&self.container.name) {
            errors.push(format!(
                "container.name: '{}' contains invalid characters (use letters, digits, hyphens, underscores, dots)",
                self.container.name
            ));
        }
        if self.container.home.as_os_str().is_empty() {
            errors.push("container.home: must not be empty".into());
        }
        if self.container.shell.trim().is_empty() {
            errors.push("container.shell: must not be empty".into());
        }
        if let Some(ref mem) = self.container.memory {
            if !is_valid_memory(mem) {
                errors.push(format!(
                    "container.memory: '{}' is not a valid memory limit (e.g. '2g', '512m')",
                    mem
                ));
            }
        }
        if let Some(ref cpus) = self.container.cpus {
            if cpus.parse::<f64>().is_err() || cpus.parse::<f64>().unwrap_or(0.0) <= 0.0 {
                errors.push(format!(
                    "container.cpus: '{}' is not a valid CPU count (e.g. '2.0', '0.5')",
                    cpus
                ));
            }
        }
        for (i, mount) in self.container.mounts.extra.iter().enumerate() {
            if !mount.contains(':') {
                errors.push(format!(
                    "container.mounts.extra[{}]: '{}' missing ':' separator (expected host:container[:options])",
                    i, mount
                ));
            }
        }
        for (key, val) in &self.container.env {
            if key.contains('\n') {
                errors.push(format!("container.env: key {:?} contains newline", key));
            }
            if val.contains('\n') {
                errors.push(format!(
                    "container.env: value for {:?} contains newline",
                    key
                ));
            }
        }

        if let Some(ref userns) = self.security.userns {
            let valid_userns = ["keep-id", "nomap", "private"];
            if !valid_userns.contains(&userns.as_str()) {
                errors.push(format!(
                    "security.userns: '{}' is invalid (expected one of: {})",
                    userns,
                    valid_userns.join(", ")
                ));
            }
        }

        // Network validation
        let valid_modes = ["host", "bridge", "none", "pasta", "slirp4netns", "private"];
        if !valid_modes.contains(&self.network.mode.as_str()) {
            errors.push(format!(
                "network.mode: '{}' is invalid (expected one of: {})",
                self.network.mode,
                valid_modes.join(", ")
            ));
        }

        for (i, port) in self.network.ports.iter().enumerate() {
            if !port.contains(':') {
                errors.push(format!(
                    "network.ports[{}]: '{}' is invalid (expected 'hostPort:containerPort' or 'ip:hostPort:containerPort')",
                    i, port
                ));
            }
        }

        if let Some(ref map) = self.integration.host_exec.allowlist {
            for (alias, path) in map {
                if !is_absolute_path(path) {
                    errors.push(format!(
                        "integration.host_exec.allowlist.{}: path '{}' is not absolute (must start with '/')",
                        alias, path
                    ));
                }
            }
        }

        if self.integration.host_exec.enabled {
            let has_allowlist = self
                .integration
                .host_exec
                .allowlist
                .as_ref()
                .is_some_and(|m| !m.is_empty());
            if !has_allowlist {
                errors.push(
                    "integration.host_exec: 'enabled' is true, but 'allowlist' is missing or empty. \
                     For security, legacy open execution is blocked; you must explicitly define \
                     allowed host commands."
                        .into(),
                );
            }
        }

        let t = &self.lifecycle.idle_timeout;
        if t != "off" {
            let (digits, suffix) = parse_duration_suffix(t);
            if digits.is_empty() || !matches!(suffix, Some('s' | 'm' | 'h')) {
                errors.push(format!(
                    "lifecycle.idle_timeout: '{}' is invalid (expected 'off', '30s', '5m', '1h')",
                    t
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(PodboxError::ConfigValidationFailed(errors.join("\n  - ")).into())
        }
    }
}

fn is_valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn is_absolute_path(s: &str) -> bool {
    s.starts_with('/')
}

/// Parse a duration string into (digit_part, suffix_char).
fn parse_duration_suffix(s: &str) -> (String, Option<char>) {
    let trimmed = s.trim();
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    let suffix = trimmed.chars().nth(digits.len());
    (digits, suffix)
}

/// Convert an idle_timeout config string to seconds.
/// Returns 0 for "off".
pub fn parse_idle_timeout_secs(s: &str) -> u64 {
    if s == "off" {
        return 0;
    }
    let (digits, suffix) = parse_duration_suffix(s);
    let value: u64 = digits.parse().unwrap_or(0);
    match suffix {
        Some('s') => value,
        Some('m') => value.saturating_mul(60),
        Some('h') => value.saturating_mul(3600),
        _ => 0,
    }
}

fn is_valid_memory(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    let suffix: String = s.chars().skip(digits.len()).collect();
    if digits.is_empty() {
        return false;
    }
    suffix.is_empty()
        || matches!(
            suffix.as_str(),
            "k" | "K" | "m" | "M" | "g" | "G" | "t" | "T"
        )
}
