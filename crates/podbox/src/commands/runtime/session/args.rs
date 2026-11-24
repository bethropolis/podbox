//! `podman exec` argv and environment construction for runtime sessions.

use std::ffi::OsString;
use std::time::Duration;

use anyhow::Result;

use podbox::config::Config;

/// Read the user's resolved PATH from the container's `/run/podbox/path`.
///
/// Returns `None` if the container is not running or the daemon hasn't
/// written the file yet (graceful fallback to Quadlet default PATH).
fn read_user_path(name: &str) -> Option<String> {
    let args = podbox::process::args(&["exec", name, "cat", "/run/podbox/path"]);
    let output =
        podbox::process::run_piped_timeout("podman", &args, Duration::from_secs(10)).ok()?;
    if !output.status.success() {
        return None;
    }
    let resolved = String::from_utf8(output.stdout).ok()?;
    let trimmed = resolved.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Insert `--env=PATH=<container path>` before the container-name slot.
///
/// Read after the box is running: the guest daemon only writes
/// `/run/podbox/path` once it is up, so reading before `ensure_running`
/// misses it on every cold start and the session falls back to the Quadlet
/// default PATH. Returns the (possibly shifted) name slot.
pub(super) fn insert_user_path(name: &str, args: &mut Vec<OsString>, name_index: usize) -> usize {
    let Some(path) = read_user_path(name) else {
        return name_index;
    };
    let index = name_index.min(args.len());
    args.splice(index..index, [OsString::from(format!("--env=PATH={path}"))]);
    name_index + 1
}

pub(super) fn append_env_args(args: &mut Vec<OsString>, env: &[String]) -> Result<()> {
    for entry in env {
        let (key, value) = entry.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("invalid environment override '{entry}': expected KEY=VALUE")
        })?;
        if key.is_empty()
            || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            || key.as_bytes()[0].is_ascii_digit()
            || value.chars().any(|c| matches!(c, '\0' | '\n' | '\r'))
        {
            anyhow::bail!("invalid environment override '{entry}': invalid key or value");
        }
        args.push(format!("--env={entry}").into());
    }
    Ok(())
}

pub(super) fn append_forwarded_env(args: &mut Vec<OsString>, config: &Config, explicit: &[String]) {
    let explicit_keys: std::collections::HashSet<&str> = explicit
        .iter()
        .filter_map(|entry| entry.split_once('=').map(|(key, _)| key))
        .collect();
    for (key, value) in std::env::vars() {
        if config.container.env.values.contains_key(&key)
            || explicit_keys.contains(key.as_str())
            || podbox::protocol::TERMINAL_ENV_KEYS.contains(&key.as_str())
        {
            continue;
        }
        let forwarded = config.container.env.forward.iter().any(|pattern| {
            if let Some(prefix) = pattern.strip_suffix('*') {
                key.starts_with(prefix)
            } else {
                key == *pattern
            }
        });
        if forwarded {
            args.push(format!("--env={key}={value}").into());
        }
    }
}

/// Forward the invoking terminal's identity into a `podman exec` session.
///
/// Podman's `-t` otherwise supplies `TERM=xterm`, which silently downgrades
/// colour and hides vendor-specific capabilities. Explicit `-e` values and
/// `[container.env]` entries win; socket-bearing variables stay on the host.
pub(super) fn append_terminal_env(args: &mut Vec<OsString>, config: &Config, explicit: &[String]) {
    for key in podbox::protocol::TERMINAL_ENV_KEYS {
        if config.container.env.values.contains_key(*key)
            || explicit
                .iter()
                .any(|entry| entry.split_once('=').is_some_and(|(k, _)| k == *key))
        {
            continue;
        }
        let Ok(value) = std::env::var(key) else {
            continue;
        };
        if value.is_empty() || value.chars().any(|c| matches!(c, '\0' | '\n' | '\r')) {
            continue;
        }
        args.push(format!("--env={key}={value}").into());
    }
}

/// Render a `podman`/`journalctl` argv for dry-run output.
pub(in crate::commands) fn args_to_string(args: &[OsString]) -> String {
    args.iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::append_env_args;

    #[test]
    fn validates_explicit_environment_overrides() {
        let mut args = Vec::new();
        assert!(append_env_args(&mut args, &["TOKEN=a=b".into()]).is_ok());
        assert_eq!(args[0].to_string_lossy(), "--env=TOKEN=a=b");
        assert!(append_env_args(&mut Vec::new(), &["BAD-NAME=x".into()]).is_err());
        assert!(append_env_args(&mut Vec::new(), &["MISSING_VALUE".into()]).is_err());
    }
}
