use crate::config::types::ProvisionStep;

use super::values::{is_valid_env_key, is_valid_provision_name, parse_provision_timeout_secs};

pub(super) fn validate_provision(steps: &[ProvisionStep]) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    if steps.len() > 64 {
        errors.push(format!(
            "provision: too many steps ({}) — at most 64",
            steps.len()
        ));
        return errors;
    }
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (i, step) in steps.iter().enumerate() {
        let prefix = format!("provision[{i}]");
        if step.name == "dotfiles" {
            errors.push(format!("{prefix}.name: 'dotfiles' is reserved"));
        } else if !is_valid_provision_name(&step.name) {
            errors.push(format!(
                "{prefix}.name: '{}' is invalid (use ^[a-z0-9][a-z0-9_-]{{0,63}}$)",
                step.name
            ));
        }
        if !seen.insert(step.name.as_str()) {
            errors.push(format!(
                "{prefix}.name: duplicate step name '{}'",
                step.name
            ));
        }
        if step.run.trim().is_empty() {
            errors.push(format!("{prefix}.run: must not be empty"));
        }
        // A present-but-empty verify would silently pass (`sh -c ""` exits
        // 0), which reads as a check that always agrees.
        if step.verify.as_ref().is_some_and(|v| v.trim().is_empty()) {
            errors.push(format!(
                "{prefix}.verify: must not be empty when set (drop the key for no postcondition)"
            ));
        }
        if parse_provision_timeout_secs(&step.timeout) == 0 {
            errors.push(format!(
                "{prefix}.timeout: '{}' is invalid (expected e.g. '30s', '5m', '1h', > 0)",
                step.timeout
            ));
        }
        for (key, value) in &step.env {
            if !is_valid_env_key(key) {
                errors.push(format!(
                    "{prefix}.env: invalid key '{key}' (use ^[A-Za-z_][A-Za-z0-9_]*$)"
                ));
            } else if key.starts_with("PODBOX_") {
                errors.push(format!(
                    "{prefix}.env: key '{key}' is reserved (PODBOX_ prefix)"
                ));
            }
            if value.chars().any(|c| matches!(c, '\0' | '\n' | '\r')) {
                errors.push(format!(
                    "{prefix}.env: value for '{key}' contains newline or NUL"
                ));
            }
        }
    }
    errors
}
