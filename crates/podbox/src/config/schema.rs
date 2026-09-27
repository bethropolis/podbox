//! `Config` — the podbox definition schema, its parse → migrate →
//! defaults → validate pipeline, and its migrations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::error::PodboxError;

/// Latest config schema version. Increment when making a backwards-incompatible
/// change, and add a migration function in `run_migrations`.
const CURRENT_SCHEMA_VERSION: u32 = 1;

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

use super::defaults::EMBEDDED_DEFAULT;
use super::types::{
    ContainerConfig, DbusConfig, DotfilesConfig, ImageConfig, IntegrationConfig, LifecycleConfig,
    NetworkConfig, SecurityConfig, StorageConfig, SystemdConfig, WaylandConfig,
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub schema_version: SchemaVersion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    pub image: ImageConfig,
    pub container: ContainerConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dotfiles: Option<DotfilesConfig>,
    #[serde(default)]
    pub integration: IntegrationConfig,
    #[serde(default)]
    pub lifecycle: LifecycleConfig,
    #[serde(default)]
    pub systemd: SystemdConfig,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub dbus: DbusConfig,
    #[serde(default)]
    pub wayland: WaylandConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default, skip_serializing_if = "storage_is_empty")]
    pub storage: StorageConfig,
}

fn storage_is_empty(storage: &StorageConfig) -> bool {
    let c = &storage.shared_caches;
    !(c.cargo || c.npm || c.pnpm || c.pip || c.ccache || c.go || c.rustup || !c.custom.is_empty())
}

impl Config {
    /// Effective D-Bus talk list.
    ///
    /// The `portal` preset no longer contributes `org.freedesktop.portal.*`
    /// here — the portal name is exposed through interface-scoped
    /// `--call=`/`--broadcast=` rules instead (see [`Self::dbus_portal_calls`]),
    /// so host-privileged portal interfaces (DynamicLauncher, Screenshot,
    /// ScreenCast, Settings, ...) are unreachable from the container.
    pub fn dbus_effective_talk(&self) -> Vec<String> {
        self.dbus.effective_talk()
    }

    /// Interface-scoped `--call=`/`--broadcast=` rules for the XDG portal name.
    ///
    /// The portal preset no longer grants `org.freedesktop.portal.*` wholesale
    /// via `--talk=`. Instead, only the interfaces actually needed by the
    /// enabled capabilities are exposed as `xdg-dbus-proxy` rules scoped to
    /// `org.freedesktop.portal.Desktop`:
    ///
    /// - `integration.notify` → `org.freedesktop.portal.Notification.*`
    /// - `integration.xdg_open` → `org.freedesktop.portal.OpenURI.*`
    ///
    /// Portals use the async Request pattern, so the `Request` interface on the
    /// `/org/freedesktop/portal/desktop/request/*` subtree is always allowed
    /// alongside (method calls for `Request.Close`, and the `Request.Response`
    /// broadcast signal that carries the actual result). A read-only
    /// `org.freedesktop.DBus.Introspectable` rule is added so GIO-based clients
    /// can introspect the service (gdbus needs the XML to parse arguments).
    pub fn dbus_portal_calls(&self) -> Vec<String> {
        let mut rules: Vec<String> = Vec::new();
        if self.integration.notify {
            rules.push(
                "--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.Notification.*@/org/freedesktop/portal/desktop"
                    .into(),
            );
        }
        if self.integration.xdg_open {
            rules.push(
                "--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.OpenURI.*@/org/freedesktop/portal/desktop"
                    .into(),
            );
        }
        if self.integration.notify || self.integration.xdg_open {
            // Portals use the async Request pattern, so the `Request` interface
            // on the `/org/freedesktop/portal/desktop/request/*` subtree is
            // always allowed alongside (method calls for `Request.Close`, and
            // the `Request.Response` signal that carries the actual result).
            rules.push(
                "--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.Request.*@/org/freedesktop/portal/desktop/request/*"
                    .into(),
            );
            rules.push(
                "--broadcast=org.freedesktop.portal.Desktop=org.freedesktop.portal.Request.*@/org/freedesktop/portal/desktop/request/*"
                    .into(),
            );
            // GIO-based clients introspect the service before calling (gdbus
            // uses the resulting XML to parse arguments). Introspection is
            // read-only (returns interface metadata) and doesn't grant any
            // method access, so it is allowed over the portal subtree.
            rules.push(
                "--call=org.freedesktop.portal.Desktop=org.freedesktop.DBus.Introspectable.*@/org/freedesktop/portal/*"
                    .into(),
            );
        }
        rules
    }

    pub fn use_dbus_proxy(&self) -> bool {
        self.integration.dbus
            && (!self.dbus_effective_talk().is_empty()
                || !self.dbus_portal_calls().is_empty()
                || !self.dbus.own.is_empty())
    }

    pub fn use_wayland_proxy(&self) -> bool {
        self.integration.wayland && self.wayland.firewall
    }

    pub fn parse(content: &str) -> Result<Config> {
        let mut config: Config = toml::from_str(content)
            .with_context(|| "failed to parse definition file".to_string())?;
        config.run_migrations();
        config.apply_defaults();
        config.validate()?;
        Ok(config)
    }

    /// Parse with `extends` resolution anchored at `path`.
    ///
    /// Relative / sibling / profile extends are resolved; the merged TOML is
    /// deserialized as a single `Config`. Source-less `parse` keeps its old
    /// behaviour for tests/embedded.
    pub fn parse_with_source(path: &std::path::Path, content: &str) -> Result<Config> {
        let merged = crate::config::extends::resolve_extends_chain(path, content)?;
        let mut config: Config = merged
            .try_into()
            .with_context(|| "failed to parse merged definition file".to_string())?;
        config.run_migrations();
        config.apply_defaults();
        config.validate()?;
        Ok(config)
    }

    pub fn load(path: &std::path::Path) -> Result<Config> {
        if !path.exists() {
            return Err(PodboxError::DefinitionNotFound {
                path: path.to_path_buf(),
            }
            .into());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read definition file '{}'", path.display()))?;
        Self::parse_with_source(path, &content)
    }

    pub fn embedded() -> Config {
        Self::parse(EMBEDDED_DEFAULT).expect("embedded default is valid TOML")
    }

    /// Run migration chain up to `CURRENT_SCHEMA_VERSION`.
    fn run_migrations(&mut self) {
        while self.schema_version.0 < CURRENT_SCHEMA_VERSION {
            match self.schema_version.0 {
                0 => {} // v0 was never released; silently bump to v1.
                1 => migrate_v1_to_v2(self),
                _ => break,
            }
            self.schema_version.0 += 1;
        }
    }

    fn apply_defaults(&mut self) {
        if self.integration.dbus
            && self.dbus.preset.is_empty()
            && self.dbus.talk.is_empty()
            && self.dbus.own.is_empty()
        {
            self.dbus.preset = "portal".into();
        }
    }
}

/// Placeholder migration — no changes from v1 to v2 yet.
fn migrate_v1_to_v2(_config: &mut Config) {}

#[cfg(test)]
mod tests;
