use crate::config::{Config, DbusPreset};

use super::super::values::{is_absolute_path, is_portal_family};

pub fn validate_host_exec_allowlist(config: &Config, errors: &mut Vec<String>) {
    if let Some(ref map) = config.integration.host_exec.allowlist {
        for (alias, entry) in map {
            let path = entry.path();
            if !is_absolute_path(path) {
                errors.push(format!(
                    "integration.host_exec.allowlist.{alias}: path '{path}' is not absolute (must start with '/')"
                ));
            }
            if let Some(secs) = entry.timeout_secs() {
                if secs < 1 {
                    errors.push(format!(
                        "integration.host_exec.allowlist.{alias}: timeout = {secs} is invalid \
                         (allowed: whole seconds >= 1; omit the key for no timeout)"
                    ));
                }
            }
            if alias.is_empty()
                || alias.contains('/')
                || alias.contains("..")
                || alias.contains('\0')
            {
                errors.push(format!(
                    "integration.host_exec.allowlist: alias '{alias}' is invalid (must not contain '/' or '..')"
                ));
            }
            const RESERVED: &[&str] = &[
                "podbox-guest",
                "podmgr-guest",
                "host-exec",
                "notify-send",
                "xdg-open",
                "podbox-clipboard",
                "podmgr-clipboard",
            ];
            if RESERVED.contains(&alias.as_str()) {
                errors.push(format!(
                    "integration.host_exec.allowlist: alias '{alias}' is reserved"
                ));
            }
        }
    }
}

pub fn validate_host_exec_enabled(config: &Config, errors: &mut Vec<String>) {
    if config.integration.host_exec.enabled {
        let has_allowlist = config
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
}

pub fn validate_dbus_talk(config: &Config, warnings: &mut Vec<String>) {
    for svc in &config.dbus.talk {
        if is_portal_family(svc) {
            warnings.push(format!(
                "warning: dbus.talk entry '{svc}' grants the container access to the full \
                 xdg-desktop-portal bus surface (DynamicLauncher, Screenshot, ScreenCast, \
                 Settings, ...). Prefer relying on the built-in interface-scoped portal rules \
                 from integration.notify / integration.xdg_open instead."
            ));
        }
    }
}

pub fn validate_dbus_switch(config: &Config, errors: &mut Vec<String>) {
    // D-Bus rules without the bus itself are dead config: with
    // `integration.dbus` off the container gets no D-Bus at all, so talk,
    // own, or a preset would be silently ignored. Fail fast instead.
    if !config.integration.dbus
        && (!config.dbus.talk.is_empty()
            || !config.dbus.own.is_empty()
            || config.dbus.preset != DbusPreset::None)
    {
        errors.push(
            "dbus: talk/own/preset rules are set but 'integration.dbus' is false, \
             so the container would get no D-Bus at all. Set 'integration.dbus = true' \
             to apply them."
                .into(),
        );
    }
}

pub fn validate_xdg_open(config: &Config, errors: &mut Vec<String>) {
    // The scheme allowlist is security-relevant input: a typo like
    // "HTTPS" would be parsed as a (never-matching) scheme and silently
    // refuse exactly the traffic the user meant to allow. Refuse it here
    // instead, while the definition file is still open for editing.
    for (i, scheme) in config
        .integration
        .xdg_open
        .allowed_schemes
        .iter()
        .enumerate()
    {
        if !crate::config::types::is_scheme_token(scheme) {
            errors.push(format!(
                "integration.xdg_open.allowed_schemes[{i}]: '{scheme}' is not a valid \
                 scheme name (lowercase letters, digits, '-', '+' or '.', starting with a letter)"
            ));
        }
    }
}
