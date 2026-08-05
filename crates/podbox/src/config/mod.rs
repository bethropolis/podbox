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

pub mod defaults;
pub mod enums;
pub mod fs;
pub mod types;
pub mod validation;

pub use defaults::EMBEDDED_DEFAULT;
pub use enums::{CapPreset, GpuMode, ImageSource, OnStop, PackageManager, XdgDirValue};
pub use fs::{
    active_context_path, clear_active_context, config_dir, expand_tilde, find_definition,
    list_configs, read_active_context, write_active_context,
};
pub use types::{
    ContainerConfig, DbusConfig, ExportConfig, HostExecConfig, ImageConfig, IntegrationConfig,
    LifecycleConfig, MountConfig, NetworkConfig, PackageConfig, RunConfig, SecurityConfig,
    SystemdConfig, WaylandConfig, XdgDirConfig,
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub schema_version: SchemaVersion,
    pub image: ImageConfig,
    pub container: ContainerConfig,
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

    pub fn load(path: &std::path::Path) -> Result<Config> {
        if !path.exists() {
            return Err(PodboxError::DefinitionNotFound {
                path: path.to_path_buf(),
            }
            .into());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read definition file '{}'", path.display()))?;
        Self::parse(&content)
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
mod tests {
    use super::*;

    #[test]
    fn test_from_str_minimal() {
        let toml = r#"
[image]
base = "fedora:41"
name = "myenv"

[container]
name = "myenv"
home = "~/containers/myenv"
"#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.image.base, "fedora:41");
        assert_eq!(cfg.image.name, "myenv");
        assert_eq!(cfg.container.name, "myenv");
        assert_eq!(cfg.container.shell, "fish");
        assert_eq!(cfg.integration.gpu, GpuMode::Auto);
        assert!(cfg.integration.wayland);
        assert!(cfg.integration.audio);
        assert!(cfg.integration.dbus);
        assert!(cfg.integration.notify);
        assert!(cfg.integration.xdg_open);
        assert!(cfg.integration.clipboard);
        assert!(!cfg.integration.host_exec.enabled);
        assert!(cfg.integration.host_exec.allowlist.is_none());
        assert!(!cfg.integration.ssh_agent);
    }

    #[test]
    fn test_home_tilde_expanded() {
        let toml = r#"
[image]
base = "fedora:41"
name = "myenv"

[container]
name = "myenv"
home = "~/containers/myenv"
"#;
        let cfg = Config::parse(toml).unwrap();
        let home = dirs::home_dir().unwrap();
        assert!(cfg.container.home.starts_with(&home));
        assert!(
            cfg.container
                .home
                .to_string_lossy()
                .contains("containers/myenv")
        );
    }

    #[test]
    fn test_on_stop_defaults_to_keep() {
        let toml = r#"
[image]
base = "fedora:41"
name = "myenv"

[container]
name = "myenv"
home = "~/containers/myenv"
"#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.lifecycle.on_stop, OnStop::Keep);
    }

    #[test]
    fn test_xdg_dirs_default_all_false() {
        let toml = r#"
[image]
base = "fedora:41"
name = "myenv"

[container]
name = "myenv"
home = "~/containers/myenv"
"#;
        let cfg = Config::parse(toml).unwrap();
        assert!(!cfg.integration.xdg_dirs.documents.is_enabled());
        assert!(!cfg.integration.xdg_dirs.downloads.is_enabled());
        assert!(!cfg.integration.xdg_dirs.pictures.is_enabled());
        assert!(!cfg.integration.xdg_dirs.music.is_enabled());
        assert!(!cfg.integration.xdg_dirs.videos.is_enabled());
        assert!(!cfg.integration.xdg_dirs.desktop.is_enabled());
    }

    #[test]
    fn test_wayland_default_is_true() {
        let toml = r#"
[image]
base = "fedora:41"
name = "myenv"

[container]
name = "myenv"
home = "~/containers/myenv"
"#;
        let cfg = Config::parse(toml).unwrap();
        assert!(cfg.integration.wayland);
        assert!(cfg.integration.audio);
    }

    #[test]
    fn test_embedded_default_parses() {
        let cfg = Config::embedded();
        assert_eq!(cfg.image.base, "fedora:44");
        assert_eq!(cfg.image.name, "podbox");
        assert_eq!(cfg.container.name, "podbox");
        assert!(cfg.integration.wayland);
        assert!(cfg.integration.audio);
        assert!(cfg.integration.dbus);
        assert_eq!(cfg.integration.gpu, GpuMode::Auto);
        assert!(!cfg.lifecycle.quadlet);
    }

    #[test]
    fn test_config_load_not_found() {
        let path = std::path::Path::new("/tmp/does_not_exist_XXXXX.toml");
        let result = Config::load(path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.downcast_ref::<PodboxError>().is_some());
    }

    #[test]
    fn test_systemd_config_parses() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[systemd]
requires = ["db.service", "cache.service"]
after = ["network.target"]
"#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.systemd.requires, vec!["db.service", "cache.service"]);
        assert_eq!(cfg.systemd.after, vec!["network.target"]);
    }

    #[test]
    fn test_visual_config_parses() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[integration]
sync_themes = true
sync_icons = true
sync_fonts = true
"#;
        let cfg = Config::parse(toml).unwrap();
        assert!(cfg.integration.sync_themes);
        assert!(cfg.integration.sync_icons);
        assert!(cfg.integration.sync_fonts);
    }

    #[test]
    fn test_dbus_config_defaults_empty() {
        let cfg = Config::embedded();
        assert_eq!(cfg.dbus.preset, "portal");
        assert!(cfg.dbus_effective_talk().is_empty());
        assert!(cfg.use_dbus_proxy());
        let calls = cfg.dbus_portal_calls();
        assert!(calls.iter().any(|r| r.contains("org.freedesktop.portal.Notification.*")));
        assert!(calls.iter().any(|r| r.contains("org.freedesktop.portal.OpenURI.*")));
    }

    #[test]
    fn test_dbus_portal_dropped_when_caps_disabled() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
preset = "portal"
[integration]
notify = false
xdg_open = false
clipboard = false
"#;
        let cfg = Config::parse(toml).unwrap();
        assert!(cfg.dbus_effective_talk().is_empty());
        assert!(cfg.dbus_portal_calls().is_empty());
        assert!(!cfg.use_dbus_proxy());
    }

    #[test]
    fn test_dbus_portal_kept_when_notify_enabled() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
preset = "portal"
[integration]
notify = true
xdg_open = false
clipboard = false
"#;
        let cfg = Config::parse(toml).unwrap();
        assert!(cfg.dbus_effective_talk().is_empty());
        assert!(cfg.use_dbus_proxy());
        let calls = cfg.dbus_portal_calls();
        assert_eq!(calls.len(), 4);
        assert!(calls[0].starts_with("--call=org.freedesktop.portal.Desktop="));
        assert!(calls[0].contains("org.freedesktop.portal.Notification.*"));
        assert!(calls[1].starts_with("--call=org.freedesktop.portal.Desktop="));
        assert!(calls[1].contains("org.freedesktop.portal.Request.*"));
        assert!(calls[2].starts_with("--broadcast=org.freedesktop.portal.Desktop="));
        assert!(calls[2].contains("org.freedesktop.portal.Request.*"));
        assert!(calls[3].starts_with("--call=org.freedesktop.portal.Desktop="));
        assert!(calls[3].contains("org.freedesktop.DBus.Introspectable.*"));
    }

    #[test]
    fn test_dbus_portal_calls_gated_by_capabilities() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
preset = "portal"
[integration]
notify = false
xdg_open = true
clipboard = false
"#;
        let cfg = Config::parse(toml).unwrap();
        let calls = cfg.dbus_portal_calls();
        assert_eq!(calls.len(), 4);
        assert!(!calls.iter().any(|r| r.contains("Notification")));
        assert!(calls.iter().any(|r| r.contains("OpenURI.*")));
        assert!(calls.iter().any(|r| r.contains("Introspectable")));
    }

    #[test]
    fn test_dbus_config_parses_talk_own() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
talk = ["org.freedesktop.Notifications", "org.mpris.MediaPlayer2.*"]
own = ["org.mpris.MediaPlayer2.podbox_app"]
"#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(
            cfg.dbus.talk,
            vec!["org.freedesktop.Notifications", "org.mpris.MediaPlayer2.*"]
        );
        assert_eq!(cfg.dbus.own, vec!["org.mpris.MediaPlayer2.podbox_app"]);
        assert!(cfg.use_dbus_proxy());
    }

    #[test]
    fn test_dbus_config_talk_only() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
talk = ["org.freedesktop.Notifications"]
"#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.dbus.talk.len(), 1);
        assert!(cfg.dbus.own.is_empty());
        assert!(cfg.use_dbus_proxy());
    }

    #[test]
    fn test_dbus_config_own_only() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
own = ["org.example.Service"]
"#;
        let cfg = Config::parse(toml).unwrap();
        assert!(cfg.dbus.talk.is_empty());
        assert_eq!(cfg.dbus.own.len(), 1);
        assert!(cfg.use_dbus_proxy());
    }

    #[test]
    fn test_invalid_toml_errors() {
        let toml = r#"
[image
base = "fedora:41"
"#;
        assert!(Config::parse(toml).is_err());
    }

    #[test]
    fn test_missing_required_fields_errors() {
        let toml = r#"
[image]
base = "fedora:41"
"#;
        assert!(Config::parse(toml).is_err());
    }

    #[test]
    fn test_network_defaults_to_private() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.network.mode, "private");
        assert!(cfg.network.ports.is_empty());
    }

    #[test]
    fn test_network_parses_mode_and_ports() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[network]
mode = "pasta"
ports = ["8080:80", "443:443"]
"#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.network.mode, "pasta");
        assert_eq!(cfg.network.ports, vec!["8080:80", "443:443"]);
    }

    #[test]
    fn test_network_invalid_mode_rejected() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[network]
mode = "macvlan"
"#;
        assert!(Config::parse(toml).is_err());
    }

    #[test]
    fn test_network_port_missing_separator_rejected() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[network]
mode = "bridge"
ports = ["8080"]
"#;
        assert!(Config::parse(toml).is_err());
    }

    #[test]
    fn test_memory_decimal_rejected() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
memory = "1.5g"
"#;
        let cfg = Config::parse(toml);
        assert!(cfg.is_err(), "decimal memory should be rejected: {:?}", cfg);
    }

    #[test]
    fn test_memory_integer_accepted() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
memory = "2g"
"#;
        assert!(Config::parse(toml).is_ok());
    }

    #[test]
    fn test_cpus_parses_valid() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
cpus = "2.0"
"#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.container.cpus.as_deref(), Some("2.0"));
    }

    #[test]
    fn test_cpus_rejects_non_positive() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
cpus = "0"
"#;
        assert!(Config::parse(toml).is_err());
    }

    #[test]
    fn test_cpus_defaults_to_none() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
        let cfg = Config::parse(toml).unwrap();
        assert!(cfg.container.cpus.is_none());
    }

    #[test]
    fn test_security_read_only_rootfs_defaults_false() {
        let cfg = Config::embedded();
        assert!(!cfg.security.read_only_rootfs);
    }

    #[test]
    fn test_security_userns_defaults_none() {
        let cfg = Config::embedded();
        assert!(cfg.security.userns.is_none());
    }

    #[test]
    fn test_security_userns_valid_modes() {
        for mode in &["keep-id", "nomap", "private"] {
            let toml = format!(
                r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[security]
userns = "{}"
"#,
                mode
            );
            assert!(
                Config::parse(&toml).is_ok(),
                "userns mode '{}' should be valid",
                mode
            );
        }
    }

    #[test]
    fn test_security_userns_invalid_mode_rejected() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[security]
userns = "invalid"
"#;
        assert!(Config::parse(toml).is_err());
    }

    #[test]
    fn test_schema_version_defaults_to_current() {
        let cfg = Config::embedded();
        assert_eq!(cfg.schema_version.as_u32(), CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_schema_version_parsed_from_toml() {
        let toml = r#"
schema_version = 1
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.schema_version.as_u32(), 1);
    }

    #[test]
    fn test_schema_version_defaults_when_omitted() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.schema_version.as_u32(), CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_schema_version_migration_bumps_old_schema() {
        let toml = r#"
schema_version = 0
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.schema_version.as_u32(), CURRENT_SCHEMA_VERSION);
    }
}
