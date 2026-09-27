//! Validation for the two cache mechanisms.
//!
//! Kept out of [`super::validate`] so the rules for a cache path live next
//! to the code that turns it into a mount, rather than in the middle of an
//! already long validation routine.

use std::collections::HashSet;
use std::path::{Component, Path};

use crate::config::{Config, SHARED_CACHE_NAMES};

/// Accept `~/relative` or an absolute path, rejecting `..` escapes, colons
/// (which would corrupt a `Volume=` line) and embedded newlines.
fn check_path(path: &str, label: &str, errors: &mut Vec<String>) {
    let relative = path.strip_prefix("~/").unwrap_or(path);
    let rooted = path.starts_with("~/") || path == "~" || path.starts_with('/');
    if path.trim().is_empty()
        || !rooted
        || path.chars().any(|c| matches!(c, '\n' | '\r' | ':'))
        || Path::new(relative)
            .components()
            .any(|c| matches!(c, Component::ParentDir))
    {
        errors.push(format!("{label}: expected '~/path' or an absolute path without '..'"));
    }
}

fn check_name(name: &str, label: &str, errors: &mut Vec<String>) {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        errors.push(format!(
            "{label}: use letters, digits, hyphens, or underscores"
        ));
    }
}

/// The container path a `mounts.extra` entry claims, if it parses.
fn mount_target(mount: &str) -> Option<&str> {
    let mut parts = mount.split(':');
    let _source = parts.next()?;
    let target = parts.next()?;
    // `host:container:opts` — options may themselves contain colons, so
    // only the first separator delimits the target.
    Some(target)
}

/// Canonical form of a container path for overlap comparison.
///
/// A hand-written mount spells the destination absolutely
/// (`/home/<user>/.cache/mbx`) while a cache config uses `~/.cache/mbx`, so
/// both are reduced to the path relative to the container home before
/// being compared.
fn container_key(path: &str) -> String {
    let relative = path.strip_prefix("~/").unwrap_or(path);
    let trimmed = relative.trim_start_matches('/');
    if let Some(rest) = trimmed.strip_prefix("home/") {
        if let Some((_user, tail)) = rest.split_once('/') {
            return tail.to_string();
        }
    }
    if let Some(tail) = trimmed.strip_prefix("root/") {
        return tail.to_string();
    }
    trimmed.to_string()
}

pub(super) fn validate_caches(config: &Config) -> Vec<String> {
    let mut errors = Vec::new();

    // ---- podbox-managed shared volumes ----
    let mut names: HashSet<&str> = HashSet::new();
    for (i, cache) in config.storage.shared_caches.custom.iter().enumerate() {
        let label = format!("storage.shared_caches.custom[{i}]");
        if SHARED_CACHE_NAMES.contains(&cache.name.as_str()) {
            errors.push(format!(
                "{label}.name: {:?} is reserved for a built-in cache",
                cache.name
            ));
        }
        check_name(&cache.name, &format!("{label}.name"), &mut errors);
        if !names.insert(cache.name.as_str()) {
            errors.push(format!("{label}.name: duplicate cache name {:?}", cache.name));
        }
        check_path(
            &cache.container_path,
            &format!("{label}.container_path"),
            &mut errors,
        );
    }

    // ---- host bind mounts ----
    let host = &config.storage.host_caches;
    let mut host_names: HashSet<&str> = HashSet::new();
    if host.mbx {
        host_names.insert("mbx");
    }
    for (i, cache) in host.custom.iter().enumerate() {
        let label = format!("storage.host_caches.custom[{i}]");
        if cache.name == "mbx" {
            errors.push(format!(
                "{label}.name: \"mbx\" is reserved for the built-in host cache"
            ));
        }
        check_name(&cache.name, &format!("{label}.name"), &mut errors);
        if !host_names.insert(cache.name.as_str()) {
            errors.push(format!("{label}.name: duplicate cache name {:?}", cache.name));
        }
        check_path(&cache.host_path, &format!("{label}.host_path"), &mut errors);
        check_path(
            &cache.container_path,
            &format!("{label}.container_path"),
            &mut errors,
        );
    }

    // A host cache and a hand-written extra mount resolving to the same
    // container path would emit two Volume= lines for one target, which
    // Podman rejects with an opaque "duplicate mount point" error. Both
    // mechanisms keep working on their own; only the clash is refused.
    let claimed: Vec<String> = config
        .container
        .mounts
        .extra
        .iter()
        .filter_map(|m| mount_target(m))
        .map(container_key)
        .filter(|k| !k.is_empty())
        .collect();
    let mut host_targets: Vec<(String, String)> = Vec::new();
    if host.mbx {
        host_targets.push((container_key("~/.cache/mbx"), "mbx".to_string()));
    }
    for cache in &host.custom {
        host_targets.push((container_key(&cache.container_path), cache.name.clone()));
    }
    for (target, name) in host_targets {
        if claimed.contains(&target) {
            errors.push(format!(
                "storage.host_caches: {name} targets {target}, which \
                 [container.mounts].extra already mounts; use one mechanism \
                 or the other, not both"
            ));
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CustomCacheConfig, HostCacheConfig};

    fn base() -> Config {
        Config::parse(
            r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/containers/env"
"#,
        )
        .expect("minimal config parses")
    }

    #[test]
    fn clean_config_has_no_cache_errors() {
        let mut config = base();
        config.storage.shared_caches.pip = true;
        config.storage.host_caches.mbx = true;
        assert!(validate_caches(&config).is_empty());
    }

    #[test]
    fn host_cache_alone_never_conflicts_with_an_unrelated_extra_mount() {
        let mut config = base();
        config.container.mounts.extra = vec![
            "/home/bet/.config/gh:/home/bet/.config/gh:ro".to_string(),
            "/home/bet/Projects:/home/bet/Projects:z".to_string(),
        ];
        config.storage.host_caches.mbx = true;
        assert!(validate_caches(&config).is_empty());
    }

    #[test]
    fn host_cache_clashing_with_an_extra_mount_is_refused() {
        let mut config = base();
        config.container.mounts.extra =
            vec!["/home/bet/.cache/mbx:/home/bet/.cache/mbx:rw,z".to_string()];
        config.storage.host_caches.mbx = true;
        let errors = validate_caches(&config);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].contains("one mechanism or the other"),
            "{errors:?}"
        );
    }

    #[test]
    fn clash_is_detected_for_custom_host_caches_too() {
        let mut config = base();
        config.container.mounts.extra =
            vec!["/home/bet/.cache/zig:/home/bet/.cache/zig:rw,z".to_string()];
        config.storage.host_caches.custom.push(HostCacheConfig {
            name: "zig".into(),
            host_path: "~/.cache/zig".into(),
            container_path: "~/.cache/zig".into(),
        });
        assert_eq!(validate_caches(&config).len(), 1);
    }

    #[test]
    fn clash_detection_survives_mount_options_after_the_target() {
        let mut config = base();
        // `z` and `rw` both appear after the target; only the first colon
        // delimits it.
        config.container.mounts.extra =
            vec!["/srv/mbx:/home/bet/.cache/mbx:rw,z".to_string()];
        config.storage.host_caches.mbx = true;
        assert_eq!(validate_caches(&config).len(), 1);
    }

    #[test]
    fn built_in_names_cannot_be_shadowed() {
        let mut config = base();
        config.storage.shared_caches.custom.push(CustomCacheConfig {
            name: "cargo".into(),
            container_path: "~/.cargo".into(),
        });
        config.storage.host_caches.custom.push(HostCacheConfig {
            name: "mbx".into(),
            host_path: "~/.cache/mbx".into(),
            container_path: "~/.cache/mbx2".into(),
        });
        let errors = validate_caches(&config);
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("built-in cache")), "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("built-in host cache")), "{errors:?}");
    }

    #[test]
    fn duplicate_names_within_a_cache_kind_are_rejected() {
        let mut config = base();
        config.storage.host_caches.custom.push(HostCacheConfig {
            name: "zig".into(),
            host_path: "~/.cache/zig".into(),
            container_path: "~/.cache/zig".into(),
        });
        config.storage.host_caches.custom.push(HostCacheConfig {
            name: "zig".into(),
            host_path: "~/.local/zig".into(),
            container_path: "~/.local/zig".into(),
        });
        assert!(validate_caches(&config).iter().any(|e| e.contains("duplicate")));
    }

    #[test]
    fn mbx_built_in_collides_with_a_duplicate_custom_name() {
        let mut config = base();
        config.storage.host_caches.mbx = true;
        config.storage.host_caches.custom.push(HostCacheConfig {
            name: "mbx".into(),
            host_path: "~/.cache/other".into(),
            container_path: "~/.cache/other".into(),
        });
        let errors = validate_caches(&config);
        assert!(errors.iter().any(|e| e.contains("duplicate")), "{errors:?}");
    }

    #[test]
    fn traversal_and_colons_are_rejected_in_every_path_kind() {
        let mut config = base();
        config.storage.host_caches.custom.push(HostCacheConfig {
            name: "zig".into(),
            host_path: "~/.cache/../../etc".into(),
            container_path: "~/.cache/zig".into(),
        });
        config.storage.shared_caches.custom.push(CustomCacheConfig {
            name: "weird".into(),
            container_path: "~/.cache/a:b".into(),
        });
        let errors = validate_caches(&config);
        assert_eq!(errors.len(), 2, "{errors:?}");
    }

    #[test]
    fn container_paths_may_be_absolute() {
        let mut config = base();
        config.storage.host_caches.custom.push(HostCacheConfig {
            name: "shared".into(),
            host_path: "/var/cache/podbox".into(),
            container_path: "/var/cache/podbox".into(),
        });
        assert!(validate_caches(&config).is_empty());
    }
}
