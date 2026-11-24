//! `Config` — the podbox definition schema and its public facade.

use serde::{Deserialize, Serialize};

use super::defaults::EMBEDDED_DEFAULT;
use super::types::{
    ContainerConfig, DbusConfig, DotfilesConfig, ImageConfig, IntegrationConfig, LifecycleConfig,
    NetworkConfig, ProvisionStep, SecurityConfig, StorageConfig, SystemdConfig, WaylandConfig,
};

mod integration_policy;
mod load;
mod normalize;
#[cfg(test)]
mod tests;

/// Latest config schema version. Increment when making a backwards-incompatible
/// change, and add a migration function in `run_migrations`.
pub(super) const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Schema version newtype with a default of 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaVersion(u32);

impl Default for SchemaVersion {
    fn default() -> Self {
        Self(CURRENT_SCHEMA_VERSION)
    }
}

impl SchemaVersion {
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

/// Read like the setup itself: what the image is, where the container
/// lives, what you bring in, how it's provisioned, what it shares, how it
/// reaches the host, and finally what runs when. Serialization follows this
/// field order, so the written TOML reads the same way.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub schema_version: SchemaVersion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    pub image: ImageConfig,
    pub container: ContainerConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dotfiles: Option<DotfilesConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provision: Vec<ProvisionStep>,
    #[serde(default, skip_serializing_if = "StorageConfig::is_empty")]
    pub storage: StorageConfig,
    #[serde(default, skip_serializing_if = "NetworkConfig::is_default")]
    pub network: NetworkConfig,
    #[serde(default, skip_serializing_if = "IntegrationConfig::is_default")]
    pub integration: IntegrationConfig,
    #[serde(default, skip_serializing_if = "DbusConfig::is_default")]
    pub dbus: DbusConfig,
    #[serde(default, skip_serializing_if = "WaylandConfig::is_default")]
    pub wayland: WaylandConfig,
    #[serde(default, skip_serializing_if = "SecurityConfig::is_default")]
    pub security: SecurityConfig,
    #[serde(default, skip_serializing_if = "LifecycleConfig::is_default")]
    pub lifecycle: LifecycleConfig,
    #[serde(default, skip_serializing_if = "SystemdConfig::is_default")]
    pub systemd: SystemdConfig,
    /// Dotted leaf paths present in the definition file (after `extends`
    /// merging, so profile values count as user values). Image labels only
    /// fill fields the user never set; see `labels::apply_defaults`.
    #[serde(skip)]
    pub explicit_keys: std::collections::HashSet<String>,
    /// Diagnostics recorded while migrations and defaults reshaped the
    /// definition (e.g. reviving a 0.8-era D-Bus intent). Printed with the
    /// validation warnings and rendered by the Studio; never serialized.
    #[serde(skip)]
    pub parse_notices: Vec<String>,
}

impl Config {
    pub fn embedded() -> Config {
        Self::parse(EMBEDDED_DEFAULT).expect("embedded default is valid TOML")
    }
}

/// Prefix of every image this crate builds: `localhost/podbox-<image.name>`.
/// The only place the literal lives, so tags can't drift apart.
pub const IMAGE_REPO_PREFIX: &str = "localhost/podbox-";

impl Config {
    /// Name used for podman containers and systemd units (`container.name`).
    ///
    /// Not the config's file name: `work.toml` may set `name = "dev"`, and the
    /// runtime name is the one that must be used everywhere after load.
    pub fn runtime_name(&self) -> &str {
        &self.container.name
    }

    /// Local image repository, `localhost/podbox-<image.name>`. Keyed on
    /// `image.name`, not `container.name`, because builds tag with it.
    pub fn image_repo(&self) -> String {
        format!("{IMAGE_REPO_PREFIX}{}", self.image.name)
    }

    /// `image_repo():latest`: the tag every build writes and Quadlet runs.
    pub fn local_image_tag(&self) -> String {
        format!("{}:latest", self.image_repo())
    }
}
