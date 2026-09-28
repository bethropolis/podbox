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

use crate::config::{Config, HostCacheConfig, HostCachesConfig, SharedCachesConfig};

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
    if caches.cargo {
        // Scoped deliberately: ~/.cargo/bin holds binaries built against one
        // distro's libc and must not cross distro boundaries.
        for (name, path) in [
            ("cargo-registry", "~/.cargo/registry"),
            ("cargo-git", "~/.cargo/git"),
        ] {
            lines.push(format!(
                "Volume=podbox-cache-{name}:{}:U",
                in_container(path, home)
            ));
        }
    }
    for (enabled, name, path) in [
        (caches.npm, "npm", "~/.npm"),
        (caches.pnpm, "pnpm", "~/.local/share/pnpm/store"),
        (caches.pip, "pip", "~/.cache/pip"),
        (caches.ccache, "ccache", "~/.cache/ccache"),
        (caches.go, "go", "~/go/pkg/mod"),
        (caches.rustup, "rustup", "~/.rustup"),
    ] {
        if enabled {
            lines.push(format!(
                "Volume=podbox-cache-{name}:{}:U",
                in_container(path, home)
            ));
        }
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
pub(super) fn emit_host_caches(lines: &mut Vec<String>, caches: &HostCachesConfig, home: &str) {
    let push = |lines: &mut Vec<String>, host_path: &str, container_path: &str| {
        lines.push(format!(
            "Volume={}:{}:rw,z",
            on_host(host_path),
            in_container(container_path, home)
        ));
    };

    if caches.mbx {
        push(lines, "~/.cache/mbx", "~/.cache/mbx");
    }
    for HostCacheConfig {
        name: _,
        host_path,
        container_path,
    } in &caches.custom
    {
        push(lines, host_path, container_path);
    }
}

/// Container-side paths a host cache would occupy, used by validation to
/// catch a path that is already claimed by `[container.mounts].extra`.
pub(crate) fn host_cache_targets(config: &Config) -> Vec<String> {
    let mut targets = Vec::new();
    if config.storage.host_caches.mbx {
        targets.push("/.cache/mbx".to_string());
    }
    for cache in &config.storage.host_caches.custom {
        targets.push(cache.container_path.clone());
    }
    targets
}

/// True when any shared or host cache is configured.
pub(crate) fn any_configured(config: &Config) -> bool {
    let shared = &config.storage.shared_caches;
    !host_cache_targets(config).is_empty()
        || shared.cargo
        || shared.npm
        || shared.pnpm
        || shared.pip
        || shared.ccache
        || shared.go
        || shared.rustup
        || !shared.custom.is_empty()
}
