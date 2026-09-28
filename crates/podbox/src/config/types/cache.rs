//! Cache storage configuration.
//!
//! Two mechanisms live here, and the difference between them is deliberately
//! small. Both offer the same set of built-in caches, both are opt-in, and
//! both custom entries name a path inside the container home. What differs is
//! only where the bytes come from:
//!
//! * [`SharedCachesConfig`] provisions podbox-managed named *volumes*
//!   (`podbox-cache-*`) shared between podbox containers, with `:U` so
//!   rootless Podman remaps ownership into the container's user namespace.
//! * [`HostCachesConfig`] bind-mounts a directory that already lives on the
//!   *host*, so the container reuses work done outside it. No `:U`: under
//!   `keep-id` the host and container UIDs already agree, and remapping a
//!   directory Podman did not create would be wrong.
//!
//! Everything they have in common — the built-in toggles, the paths those
//! caches occupy, and the rule for turning a toggle into mounts — is defined
//! once in [`BuiltinCaches`] and shared, so the two lists cannot drift apart.
//! Each config is that struct plus its own `custom` entries.

use serde::{Deserialize, Serialize};

use crate::config::defaults::is_false;

/// Every built-in cache and the paths it occupies in the container home.
///
/// A cache with more than one path is split across several mounts, which is
/// why `cargo` appears twice in the expansion: `~/.cargo/bin` holds binaries
/// built against one distro's libc and must not cross distro boundaries.
pub const BUILTIN_CACHES: &[(&str, &[&str])] = &[
    ("cargo", &["~/.cargo/registry", "~/.cargo/git"]),
    ("npm", &["~/.npm"]),
    ("pnpm", &["~/.local/share/pnpm/store"]),
    ("pip", &["~/.cache/pip"]),
    ("ccache", &["~/.cache/ccache"]),
    ("go", &["~/go/pkg/mod"]),
    // Compiler binaries, same reasoning as `~/.cargo/bin`.
    ("rustup", &["~/.rustup"]),
    ("mbx", &["~/.cache/mbx"]),
];

/// One mount belonging to a built-in cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinCachePath {
    /// The config key that requested it, e.g. `cargo`.
    pub cache: &'static str,
    /// Name for the mount, e.g. `cargo-registry`. Equal to `cache` unless
    /// the cache spans several paths.
    pub mount: &'static str,
    /// Path inside the container home, `~/`-prefixed.
    pub container_path: &'static str,
}

/// Expand the built-in table into one entry per mount, in order.
fn all_builtin_paths() -> Vec<BuiltinCachePath> {
    BUILTIN_CACHES
        .iter()
        .flat_map(|(cache, paths)| {
            let multi = paths.len() > 1;
            paths.iter().map(move |path| BuiltinCachePath {
                cache,
                mount: if multi {
                    match *path {
                        "~/.cargo/registry" => "cargo-registry",
                        "~/.cargo/git" => "cargo-git",
                        _ => cache,
                    }
                } else {
                    cache
                },
                container_path: path,
            })
        })
        .collect()
}

/// The built-in cache toggles, shared by both mechanisms.
///
/// `Copy` so [`enabled_paths`](Self::enabled_paths) hands back a plain list
/// without cloning the config around.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuiltinCaches {
    #[serde(default, skip_serializing_if = "is_false")]
    pub cargo: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub npm: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub pnpm: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub pip: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ccache: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub go: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub rustup: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub mbx: bool,
}

impl BuiltinCaches {
    /// The mounts the requested built-ins need, in declaration order.
    pub fn enabled_paths(&self) -> Vec<BuiltinCachePath> {
        all_builtin_paths()
            .into_iter()
            .filter(|p| self.is_enabled(p.cache))
            .collect()
    }

    fn is_enabled(&self, cache: &str) -> bool {
        match cache {
            "cargo" => self.cargo,
            "npm" => self.npm,
            "pnpm" => self.pnpm,
            "pip" => self.pip,
            "ccache" => self.ccache,
            "go" => self.go,
            "rustup" => self.rustup,
            "mbx" => self.mbx,
            _ => false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// Names a `custom` entry may not use, because it would shadow a built-in.
pub fn builtin_cache_names() -> impl Iterator<Item = &'static str> {
    BUILTIN_CACHES.iter().map(|(name, _)| *name)
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct StorageConfig {
    #[serde(default, skip_serializing_if = "SharedCachesConfig::is_empty")]
    pub shared_caches: SharedCachesConfig,
    #[serde(default, skip_serializing_if = "HostCachesConfig::is_empty")]
    pub host_caches: HostCachesConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct SharedCachesConfig {
    #[serde(flatten)]
    pub builtins: BuiltinCaches,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<CustomCacheConfig>,
}

impl SharedCachesConfig {
    pub fn enabled_paths(&self) -> Vec<BuiltinCachePath> {
        self.builtins.enabled_paths()
    }

    pub fn is_empty(&self) -> bool {
        self.builtins.is_empty() && self.custom.is_empty()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct HostCachesConfig {
    #[serde(flatten)]
    pub builtins: BuiltinCaches,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<HostCacheConfig>,
}

impl StorageConfig {
    pub fn is_empty(&self) -> bool {
        self.shared_caches.is_empty() && self.host_caches.is_empty()
    }
}

impl HostCachesConfig {
    pub fn enabled_paths(&self) -> Vec<BuiltinCachePath> {
        self.builtins.enabled_paths()
    }

    pub fn is_empty(&self) -> bool {
        self.builtins.is_empty() && self.custom.is_empty()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CustomCacheConfig {
    pub name: String,
    pub container_path: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HostCacheConfig {
    pub name: String,
    /// Path on the host, relative to the host user's home (`~/…`) or
    /// absolute.
    pub host_path: String,
    /// Destination inside the container home (`~/…`) or absolute.
    pub container_path: String,
}
