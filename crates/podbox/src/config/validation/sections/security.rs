use crate::config::{Config, SecretEntry, SecretSource, SecretType};

use super::super::values::is_valid_env_key;

pub fn validate_secrets(config: &Config, errors: &mut Vec<String>) {
    // systemd-credential secrets become LoadCredential= names and
    // /run/secrets/<name> file names: reject path separators and
    // parent references, require the host-side path, and require a
    // valid variable name when the target becomes Environment=KEY=….
    for (i, entry) in config.security.secrets.iter().enumerate() {
        let (name, secret_type, target, source, path) = match entry {
            SecretEntry::Simple(name) => {
                (name, &SecretType::Env, None, &SecretSource::Podman, None)
            }
            SecretEntry::Detailed {
                name,
                secret_type,
                target,
                source,
                path,
                ..
            } => (name, secret_type, target.as_ref(), source, path.as_ref()),
        };
        let prefix = format!("security.secrets[{i}]");
        if name.is_empty()
            || name.contains('/')
            || name.contains('\\')
            || name.split('/').any(|part| part == "..")
        {
            errors.push(format!(
                "{prefix}.name: '{name}' is not a safe credential filename (no '/', '\\\\' or '..')"
            ));
        }
        if *source == SecretSource::Systemd {
            match path {
                Some(p) if !p.trim().is_empty() => {}
                _ => errors.push(format!(
                    "{prefix}.path: required with source = \"systemd\" (host-side credential file)"
                )),
            }
            if *secret_type == SecretType::Env
                && let Some(t) = target
                && !is_valid_env_key(t)
            {
                errors.push(format!(
                    "{prefix}.target: '{t}' must be a valid environment variable name"
                ));
            }
        }
    }
}
