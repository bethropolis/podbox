use crate::config::Config;

use super::super::values::parse_duration_secs;

pub fn validate_on_stop(config: &Config, warnings: &mut Vec<String>) {
    // `lifecycle.on_stop` is deprecated: Quadlet runs containers with
    // `--rm --replace`, so every box is ephemeral and there is nothing to
    // keep. Warn when set explicitly, but keep parsing it.
    if config.explicit_keys.contains("lifecycle.on_stop") {
        warnings.push(
            "warning: lifecycle.on_stop is deprecated and has no effect: Quadlet containers \
             are always ephemeral; state lives in the home volume and snapshots."
                .into(),
        );
    }
}

pub fn validate_durations(config: &Config, errors: &mut Vec<String>) {
    let t = &config.lifecycle.idle_timeout;
    if t != "off" {
        if let Err(e) = parse_duration_secs(t) {
            errors.push(format!(
                "lifecycle.idle_timeout: '{t}' is invalid ({e}; expected 'off', '30s', '5m', '1h')"
            ));
        }
    }
    if let Err(e) = parse_duration_secs(&config.image.pull_retry_delay) {
        errors.push(format!(
            "image.pull_retry_delay: '{}' is invalid ({e}; expected e.g. '5s')",
            config.image.pull_retry_delay
        ));
    }
}
