//! Secret credential checks: podman store membership and systemd passthrough.

use podbox::config::Config;

use super::super::Check;

pub(crate) fn check_secrets(config: &Config) -> Vec<Check> {
    let mut out = Vec::new();
    // ── secrets: verify podman secrets exist ──
    if !config.security.secrets.is_empty() {
        let output = std::process::Command::new("podman")
            .args(["secret", "ls", "--format", "{{.Name}}"])
            .output();
        let available: std::collections::HashSet<String> = match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            _ => std::collections::HashSet::new(),
        };
        for secret in &config.security.secrets {
            let (name, source) = match secret {
                podbox::config::SecretEntry::Simple(n) => {
                    (n.as_str(), podbox::config::SecretSource::Podman)
                }
                podbox::config::SecretEntry::Detailed { name, source, .. } => {
                    (name.as_str(), *source)
                }
            };
            let label = format!("secret: {name}");
            if source == podbox::config::SecretSource::Systemd {
                out.push(Check::new(&label, "pass", "systemd credential passthrough"));
            } else if available.contains(name) {
                out.push(Check::new(&label, "pass", "found in podman secret store"));
            } else {
                out.push(Check::new(
                    &label,
                    "fail",
                    format!("missing — create with `podman secret create {name} -`"),
                ));
            }
        }
    }
    out
}
