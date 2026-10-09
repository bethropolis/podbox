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

use crate::config::{Config, HostCachesConfig, SharedCachesConfig};
use crate::env::HostEnv;

/// Expand a `~/`-prefixed path against a container home, leaving absolute
/// paths untouched.
fn in_container(path: &str, home: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => format!("{home}/{rest}"),
        None if path == "~" => home.to_string(),
        None => path.to_string(),
    }
}

/// Expand a `~/`-prefixed path against the *host* home, expressed with
/// systemd's `%h` so the unit stays correct when the host username and the
/// container username differ.
fn on_host(path: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => format!("%h/{rest}"),
        None if path == "~" => "%h".to_string(),
        None => path.to_string(),
    }
}

/// Emit `Volume=` lines for podbox-managed shared cache volumes.
pub(super) fn emit_shared_caches(lines: &mut Vec<String>, caches: &SharedCachesConfig, home: &str) {
    for path in caches.enabled_paths() {
        lines.push(format!(
            "Volume=podbox-cache-{}:{}:U",
            path.mount,
            in_container(path.container_path, home)
        ));
    }
    for cache in &caches.custom {
        lines.push(format!(
            "Volume=podbox-cache-{}:{}:U",
            cache.name,
            in_container(&cache.container_path, home)
        ));
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
        lines.push(format!(
            "Volume={}:{}:rw,z",
            on_host(path.container_path),
            in_container(path.container_path, home)
        ));
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
        lines.push(format!(
            "Volume={}:{}:rw,z",
            on_host(&cache.host_path),
            in_container(&cache.container_path, home)
        ));
    }
}

/// Render a container path the way the host knows it, for the warning.
fn host_display(container_path: &str) -> String {
    on_host(container_path).replace("%h", "~")
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
