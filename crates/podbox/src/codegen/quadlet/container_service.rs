use super::value;
use crate::config::Config;

pub(super) fn emit_service_section(
    lines: &mut Vec<String>,
    config: &Config,
    warnings: &mut Vec<String>,
) {
    lines.push("[Service]".into());
    value::push_raw(lines, warnings, "Slice", &config.container.slice);
    value::push_raw(
        lines,
        warnings,
        "CPUWeight",
        &config.container.cpu_weight.to_string(),
    );
    lines.push("Restart=on-failure".into());
    lines.push("RestartSec=2s".into());
    emit_service_credentials(lines, config, warnings);
    lines.push(String::new());
}

/// `LoadCredential=` lines for systemd-credential secrets.
///
/// Without these, `%d` (`$CREDENTIALS_DIRECTORY`) is empty and the
/// `Volume=%d/…` lines in `[Container]` mount nothing. The host-side `path`
/// is required by validation; `encrypted` selects the encrypted variant.
fn emit_service_credentials(lines: &mut Vec<String>, config: &Config, warnings: &mut Vec<String>) {
    use crate::config::{SecretEntry, SecretSource};
    for secret in &config.security.secrets {
        let (name, path, encrypted) = match secret {
            SecretEntry::Simple(_) => continue,
            SecretEntry::Detailed {
                name,
                source,
                path,
                encrypted,
                ..
            } => {
                if *source != SecretSource::Systemd {
                    continue;
                }
                (name, path, encrypted)
            }
        };
        match (value::raw(name), path.as_deref().map(value::raw)) {
            (Ok(name_escaped), Some(Ok(path_escaped))) => {
                let key = if *encrypted {
                    "LoadCredentialEncrypted"
                } else {
                    "LoadCredential"
                };
                value::push_composed(lines, format!("{key}={name_escaped}:{path_escaped}"));
            }
            _ => warnings.push(format!(
                "skipping credential '{name}': invalid name or path"
            )),
        }
    }
}
