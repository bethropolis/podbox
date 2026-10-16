//! `Volume=` lines for the two cache mechanisms.
//!
//! Shared caches ([`SharedCachesConfig`]) are podbox-managed named volumes
//! tagged `:U` so rootless Podman remaps ownership into the container's
//! user namespace — necessary because the volume is created by Podman
//! rather than inherited from the host.
//!
//! Host caches ([`HostCachesConfig`]) are bind mounts of a directory that
//! already lives on the host, so they carry the `:z` SELinux label and
//! deliberately *not* `:U`: under `keep-id` the host and container UIDs
//! already agree, and remapping an existing host directory would be wrong.

use super::value;
use crate::config::{Config, HostCachesConfig, SharedCachesConfig};
use crate::env::HostEnv;

/// Expand a `~/`-prefixed path against a container home, leaving absolute
/// paths untouched. Only the user-controlled remainder goes through the
/// writer; the container `home` itself carries systemd's intentional `%u`
/// specifier and must stay single.
fn in_container_escaped(path: &str, home: &str) -> Result<String, value::ValueError> {
    match path.strip_prefix("~/") {
        Some(rest) => Ok(format!("{home}/{}", value::raw(rest)?)),
        None if path == "~" => Ok(home.to_string()),
        None => value::raw(path),
    }
}

/// Expand a `~/`-prefixed path against the *host* home, expressed with
/// systemd's `%h` so the unit stays correct when the host username and the
/// container username differ. The user-controlled remainder is escaped; the
/// emitted `%h` is the single intentional specifier.
fn on_host_escaped(path: &str) -> Result<String, value::ValueError> {
    match path.strip_prefix("~/") {
        Some(rest) => Ok(format!("%h/{}", value::raw(rest)?)),
        None if path == "~" => Ok("%h".to_string()),
        None => value::raw(path),
    }
}

/// Emit `Volume=` lines for podbox-managed shared cache volumes.
pub(super) fn emit_shared_caches(
    lines: &mut Vec<String>,
    caches: &SharedCachesConfig,
    home: &str,
    warnings: &mut Vec<String>,
) {
    for path in caches.enabled_paths() {
        match (
            value::raw(path.mount),
            in_container_escaped(path.container_path, home),
        ) {
            (Ok(mount), Ok(target)) => {
                value::push_composed(lines, format!("Volume=podbox-cache-{mount}:{target}:U"));
            }
            _ => warnings.push(format!(
                "skipping shared cache '{}': invalid characters in path",
                path.mount
            )),
        }
    }
    for cache in &caches.custom {
        match (
            value::raw(&cache.name),
            in_container_escaped(&cache.container_path, home),
        ) {
            (Ok(mount), Ok(target)) => {
                value::push_composed(lines, format!("Volume=podbox-cache-{mount}:{target}:U"));
            }
            _ => warnings.push(format!(
                "skipping shared cache '{}': invalid characters in path",
                cache.name
            )),
        }
    }
}

/// Emit `Volume=` lines for host bind-mounted caches.
///
/// A cache whose host directory does not exist is skipped with a warning
/// rather than emitted: there is nothing on the host to reuse, and podman
/// fails to start the container outright when a bind source is missing.
/// `env.host_cache_dirs` is `None` for a preview host, so nothing is
/// skipped there.
pub(super) fn emit_host_caches(
    lines: &mut Vec<String>,
    caches: &HostCachesConfig,
    home: &str,
    env: &HostEnv,
    warnings: &mut Vec<String>,
) {
    // A built-in is the same relative path on both sides, so the host path
    // is just the container path with the container home removed.
    let probed = env.host_cache_dirs.as_ref();
    for path in caches.enabled_paths() {
        if let Some(present) = probed
            && !present.iter().any(|p| p == path.container_path)
        {
            warnings.push(format!(
                "storage.host_caches: '{}' skipped -- {} does not exist on the host",
                path.cache,
                host_display(path.container_path)
            ));
            continue;
        }
        match (
            on_host_escaped(path.container_path),
            in_container_escaped(path.container_path, home),
        ) {
            (Ok(source), Ok(target)) => {
                value::push_composed(lines, format!("Volume={source}:{target}:rw,z"));
            }
            _ => warnings.push(format!(
                "skipping host cache '{}': invalid characters in path",
                path.cache
            )),
        }
    }
    for cache in &caches.custom {
        if let Some(present) = probed
            && !present.contains(&cache.host_path)
        {
            warnings.push(format!(
                "storage.host_caches: '{}' skipped -- {} does not exist on the host",
                cache.name, cache.host_path
            ));
            continue;
        }
        match (
            on_host_escaped(&cache.host_path),
            in_container_escaped(&cache.container_path, home),
        ) {
            (Ok(source), Ok(target)) => {
                value::push_composed(lines, format!("Volume={source}:{target}:rw,z"));
            }
            _ => warnings.push(format!(
                "skipping host cache '{}': invalid characters in path",
                cache.name
            )),
        }
    }
}

/// Render a container path the way the host knows it, for the warning.
fn host_display(container_path: &str) -> String {
    match container_path.strip_prefix("~/") {
        Some(rest) => format!("~/{rest}"),
        None => container_path.to_string(),
    }
}

/// Container-side paths a host cache would occupy, used by validation to
/// catch a path that is already claimed by `[container.mounts].extra`.
pub(crate) fn host_cache_targets(config: &Config) -> Vec<String> {
    let mut targets: Vec<String> = config
        .storage
        .host_caches
        .enabled_paths()
        .iter()
        .map(|p| comparable(p.container_path))
        .collect();
    targets.extend(
        config
            .storage
            .host_caches
            .custom
            .iter()
            .map(|c| comparable(&c.container_path)),
    );
    targets
}

/// Reduce a `~/`-relative container path to the part under the home, so a
/// hand-written absolute mount and a config path can be compared.
fn comparable(path: &str) -> String {
    path.strip_prefix("~/").unwrap_or(path).to_string()
}

/// True when any shared or host cache is configured.
pub(crate) fn any_configured(config: &Config) -> bool {
    !config.storage.shared_caches.enabled_paths().is_empty()
        || !config.storage.shared_caches.custom.is_empty()
        || !host_cache_targets(config).is_empty()
}
