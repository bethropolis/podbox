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
    ContainerConfig, DbusConfig, DbusPreset, DotfilesConfig, ImageConfig, IntegrationConfig,
    LifecycleConfig, NetworkConfig, ProvisionStep, SecurityConfig, StorageConfig, SystemdConfig,
    WaylandConfig,
};

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
        if self.integration.xdg_open.enabled {
            rules.push(
                "--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.OpenURI.*@/org/freedesktop/portal/desktop"
                    .into(),
            );
        }
        if self.integration.notify || self.integration.xdg_open.enabled {
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
            // Nearly every GTK app reads theme preferences via Settings.Read
            // at startup; without this it falls back with a warning. Read-only
            // like introspection — no portal privilege beyond preferences.
            rules.push(
                "--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.Settings.*@/org/freedesktop/portal/desktop"
                    .into(),
            );
        }
        rules
    }

    pub fn use_dbus_proxy(&self) -> bool {
        self.integration.dbus
            && (self.dbus.preset != DbusPreset::None
                || !self.dbus_effective_talk().is_empty()
                || !self.dbus_portal_calls().is_empty()
                || !self.dbus.own.is_empty())
    }

    pub fn use_wayland_proxy(&self) -> bool {
        self.integration.wayland && self.wayland.firewall
    }

    pub fn parse(content: &str) -> Result<Config> {
        let (config, _) = Self::parse_with_warnings(content)?;
        Ok(config)
    }

    /// Parse, keeping non-fatal validation diagnostics alongside the config.
    /// The wasm/Studio path uses this so warnings render in the UI instead
    /// of going to stderr (which panics on wasm targets).
    pub fn parse_with_warnings(content: &str) -> Result<(Config, Vec<String>)> {
        let value: toml::Value = toml::from_str(content)
            .with_context(|| "failed to parse definition file".to_string())?;
        let config = Self::from_merged(value)?;
        let (warnings, result) = config.validate_with_warnings();
        result.map(|()| (config, warnings))
    }

    /// Parse with `extends` resolution anchored at `path`.
    ///
    /// Relative / sibling / profile extends are resolved; the merged TOML is
    /// deserialized as a single `Config`. Source-less `parse` keeps its old
    /// behaviour for tests/embedded.
    pub fn parse_with_source(path: &std::path::Path, content: &str) -> Result<Config> {
        let merged = crate::config::extends::resolve_extends_chain(path, content)?;
        let config = Self::from_merged(merged)?;
        config.validate()?;
        Ok(config)
    }

    /// Deserialize, migrate and default a merged TOML document without
    /// validating. The two entry points [`Self::parse_with_source`] (hard
    /// validate) and [`Self::load_lenient`] (validate for cleanup) share this
    /// so both see identical migrations and defaults.
    fn from_merged(value: toml::Value) -> Result<Config> {
        // A newer schema may add keys this version denies: report the
        // upgrade hint, not a confusing "unknown field" error.
        check_schema_version(&value)?;
        let explicit_keys = explicit_leaf_paths(&value);
        let mut config: Config = value
            .try_into()
            .with_context(|| "failed to parse definition file".to_string())?;
        config.explicit_keys = explicit_keys;
        config.run_migrations();
        config.apply_defaults();
        Ok(config)
    }

    /// Load for cleanup commands (`remove`): definition-neutral bookkeeping
    /// that must work on a legacy definition the validator rejects. Only
    /// *validation* failures are demoted to printed warnings — structural
    /// failures (unreadable file, unknown keys, newer schema) still refuse,
    /// because working around those could delete more than the container
    /// this command names.
    pub fn load_lenient(path: &std::path::Path) -> Result<Config> {
        if !path.exists() {
            return Err(PodboxError::DefinitionNotFound {
                path: path.to_path_buf(),
            }
            .into());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read definition file '{}'", path.display()))?;
        let merged = crate::config::extends::resolve_extends_chain(path, &content)?;
        let config = Self::from_merged(merged)?;
        let (warnings, result) = config.validate_with_warnings();
        // Nothing printed yet (this path never calls the auto-printing
        // `validate`), so surface every diagnostic here: the parse warnings,
        // and a demoted error verdict when the definition would not load
        // under the strict rules. Printing is deliberate: a cleanup running
        // against a knowingly-invalid definition must say so.
        #[cfg(feature = "cli")]
        for line in warnings {
            crate::config::validation::note_parse_warning(line);
        }
        // wasm has no stderr anyway; the verdict simply stops being fatal.
        #[cfg(not(feature = "cli"))]
        let _ = warnings;
        if let Err(err) = result {
            #[cfg(feature = "cli")]
            crate::config::validation::note_parse_warning(format!(
                "not applying strict validation rules for removal: {err:#}"
            ));
            #[cfg(not(feature = "cli"))]
            let _ = err;
        }
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
        // 0.9.0 made the integrations opt-in. Definitions written by 0.8.x
        // never say `integration.dbus` (the old emitter skipped the key while
        // it defaulted on) but do carry `[dbus]` rules: the rules carry the
        // old on-by-default intent, so an unstated switch keeps the bus on.
        // An explicit `integration.dbus = false` next to rules stays a
        // validation error — that is a genuine contradiction.
        let dbus_rules = !self.dbus.talk.is_empty()
            || !self.dbus.own.is_empty()
            || self.dbus.preset != DbusPreset::None;
        if !self.integration.dbus && dbus_rules && !self.explicit_keys.contains("integration.dbus")
        {
            self.integration.dbus = true;
            self.parse_notices.push(
                "dbus: [dbus] rules imply the bus is wanted, so 'integration.dbus' was enabled \
                 (podbox ≤0.8 had it on by default). To keep the bus off, set \
                 'integration.dbus = false' and remove the [dbus] rules."
                    .to_string(),
            );
        }

        if self.integration.dbus
            && self.dbus.preset == DbusPreset::None
            && self.dbus.talk.is_empty()
            && self.dbus.own.is_empty()
        {
            self.dbus.preset = DbusPreset::Portal;
        }
    }
}

/// Reject configs written by a newer podbox: fields this version does
/// not know may be ignored silently, producing a box that differs from
/// what the file describes. Older versions still run the migration chain.
fn check_schema_version(value: &toml::Value) -> Result<()> {
    if let Some(seen) = value
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        && seen > i64::from(CURRENT_SCHEMA_VERSION)
    {
        anyhow::bail!(
            "config requires podbox schema {seen}; this podbox supports schema {CURRENT_SCHEMA_VERSION} — upgrade podbox"
        );
    }
    Ok(())
}

/// Dotted leaf paths present in a TOML document (`integration.audio`,
/// `integration.xdg_dirs.downloads`, `image.packages.install[0]`).
/// Used to tell user-set fields apart from defaults so image labels can
/// only fill in what the user never set.
fn explicit_leaf_paths(value: &toml::Value) -> std::collections::HashSet<String> {
    fn walk(prefix: String, value: &toml::Value, out: &mut std::collections::HashSet<String>) {
        match value {
            toml::Value::Table(table) => {
                for (key, child) in table {
                    let child_prefix = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    walk(child_prefix, child, out);
                }
            }
            toml::Value::Array(items) => {
                out.insert(prefix.clone());
                for (i, child) in items.iter().enumerate() {
                    walk(format!("{prefix}[{i}]"), child, out);
                }
            }
            _ => {
                out.insert(prefix);
            }
        }
    }
    let mut out = std::collections::HashSet::new();
    walk(String::new(), value, &mut out);
    out
}

/// Placeholder migration — no changes from v1 to v2 yet.
fn migrate_v1_to_v2(_config: &mut Config) {}

#[cfg(test)]
mod tests;

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
