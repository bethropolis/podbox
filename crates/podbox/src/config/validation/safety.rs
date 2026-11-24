use crate::config::{Config, SecretEntry};

/// Validate strings that reach a unit line or Containerfile `RUN` layer.
///
/// A newline would inject an extra directive or build instruction, so each
/// violation names its field path in the same order as the config surfaces.
pub(super) fn validate_safety(config: &Config) -> Vec<String> {
    let mut errors = Vec::new();
    let mut fields: Vec<(String, &str)> = Vec::new();
    for (i, dep) in config.systemd.requires.iter().enumerate() {
        fields.push((format!("systemd.requires[{i}]"), dep));
    }
    for (i, dep) in config.systemd.after.iter().enumerate() {
        fields.push((format!("systemd.after[{i}]"), dep));
    }
    if let Some(ref cmd) = config.container.reload_cmd {
        fields.push(("container.reload_cmd".into(), cmd));
    }
    for (i, cap) in config.security.cap_add.iter().enumerate() {
        fields.push((format!("security.cap_add[{i}]"), cap));
    }
    if let Some(ref profile) = config.security.seccomp {
        fields.push(("security.seccomp".into(), profile));
    }
    if let Some(ref profile) = config.security.apparmor {
        fields.push(("security.apparmor".into(), profile));
    }
    for (i, mount) in config.container.mounts.extra.iter().enumerate() {
        fields.push((format!("container.mounts.extra[{i}]"), mount));
    }
    for (i, cmd) in config.image.run.commands.iter().enumerate() {
        fields.push((format!("image.run.commands[{i}]"), cmd));
    }
    for (i, pkg) in config.image.packages.install.iter().enumerate() {
        fields.push((format!("image.packages.install[{i}]"), pkg));
    }
    for (i, entry) in config.security.secrets.iter().enumerate() {
        let (name, target, mode) = match entry {
            SecretEntry::Simple(name) => (name, None, None),
            SecretEntry::Detailed {
                name, target, mode, ..
            } => (name, target.as_ref(), mode.as_ref()),
        };
        fields.push((format!("security.secrets[{i}].name"), name));
        if let Some(t) = target {
            fields.push((format!("security.secrets[{i}].target"), t));
        }
        if let Some(m) = mode {
            fields.push((format!("security.secrets[{i}].mode"), m));
        }
    }
    for (i, svc) in config.dbus.talk.iter().enumerate() {
        fields.push((format!("dbus.talk[{i}]"), svc));
    }
    for (i, svc) in config.dbus.own.iter().enumerate() {
        fields.push((format!("dbus.own[{i}]"), svc));
    }
    for (i, port) in config.network.ports.iter().enumerate() {
        fields.push((format!("network.ports[{i}]"), port));
    }
    fields.push(("image.base".into(), &config.image.base));
    if let Some(ref image_ref) = config.image.image_ref {
        fields.push(("image.image".into(), image_ref));
    }
    fields.push((
        "image.pull_retry_delay".into(),
        &config.image.pull_retry_delay,
    ));
    fields.push(("container.shell".into(), &config.container.shell));
    for (path, value) in &fields {
        if value.chars().any(|c| matches!(c, '\0' | '\n' | '\r')) {
            errors.push(format!(
                "{path}: must not contain NUL, carriage return, or newline"
            ));
        }
    }
    errors
}
