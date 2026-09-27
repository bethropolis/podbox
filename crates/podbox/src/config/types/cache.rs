//! Cache storage configuration.
//!
//! Two independent kinds of cache live here, and they are deliberately
//! different mechanisms:
//!
//! * [`SharedCachesConfig`] provisions podbox-managed named *volumes*
//!   (`podbox-cache-*`) shared between podbox containers. They are
//!   independent of the host's own caches.
//! * [`HostCachesConfig`] bind-mounts a cache that already lives on the
//!   *host*, so a container reuses the work done outside it. These are
//!   plain bind mounts: no `:U` ownership remapping, because under
//!   `keep-id` the host and container UIDs already agree.
//!
//! Both are opt-in. Nothing is provisioned unless the config asks for it.

use serde::{Deserialize, Serialize};

use crate::config::defaults::is_false;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct StorageConfig {
    #[serde(default)]
    pub shared_caches: SharedCachesConfig,
    #[serde(default, skip_serializing_if = "HostCachesConfig::is_empty")]
    pub host_caches: HostCachesConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct SharedCachesConfig {
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<CustomCacheConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CustomCacheConfig {
    pub name: String,
    pub container_path: String,
}

/// Names of the built-in [`SharedCachesConfig`] toggles, which
/// [`CustomCacheConfig::name`] may not shadow.
pub const SHARED_CACHE_NAMES: [&str; 7] =
    ["cargo", "npm", "pnpm", "pip", "ccache", "go", "rustup"];

/// Host build caches bind-mounted into the container.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct HostCachesConfig {
    /// Mr Boxington's store, at `~/.cache/mbx` on both sides.
    #[serde(default, skip_serializing_if = "is_false")]
    pub mbx: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<HostCacheConfig>,
}

impl HostCachesConfig {
    pub fn is_empty(&self) -> bool {
        !self.mbx && self.custom.is_empty()
    }
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
