use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageSource {
    Build { base: String },
    Prebuilt { ref_str: String },
}

impl ImageSource {
    pub fn is_prebuilt(&self) -> bool {
        matches!(self, Self::Prebuilt { .. })
    }

    pub fn is_build(&self) -> bool {
        matches!(self, Self::Build { .. })
    }
}

/// Type-safe package manager identifier.
///
/// Replaces raw `&str` dispatch everywhere.  Always use the enum rather
/// than hard-coding strings like `"dnf"` / `"apt"`.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, strum::VariantNames,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum PackageManager {
    #[default]
    Dnf,
    Apt,
    Pacman,
    Apk,
    Zypper,
}

impl PackageManager {
    /// Canonical display name (e.g. `"dnf"`, `"apt"`).
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Dnf => "dnf",
            Self::Apt => "apt",
            Self::Pacman => "pacman",
            Self::Apk => "apk",
            Self::Zypper => "zypper",
        }
    }

    /// Parse a string into a `PackageManager`, returning `None` for unknown
    /// values (callers fall back to distro detection).
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "dnf" => Some(Self::Dnf),
            "apt" => Some(Self::Apt),
            "pacman" => Some(Self::Pacman),
            "apk" => Some(Self::Apk),
            "zypper" => Some(Self::Zypper),
            _ => None,
        }
    }
}

impl std::fmt::Display for PackageManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for PackageManager {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_opt(s.trim()).ok_or_else(|| format!("unknown package manager: {s}"))
    }
}

/// Container network mode. The TOML spellings are unchanged (`rename_all`
/// keeps them lowercase), so existing files parse as before; typos now
/// fail at parse time with the valid list instead of reaching validation
/// or podman.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, strum::VariantNames,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum NetworkMode {
    Host,
    Bridge,
    None,
    #[default]
    Pasta,
    Slirp4netns,
    Private,
}

impl NetworkMode {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Bridge => "bridge",
            Self::None => "none",
            Self::Pasta => "pasta",
            Self::Slirp4netns => "slirp4netns",
            Self::Private => "private",
        }
    }
}

impl std::fmt::Display for NetworkMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// User-namespace mode for the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, strum::VariantNames)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum UsernsMode {
    KeepId,
    Nomap,
    Private,
}

impl UsernsMode {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::KeepId => "keep-id",
            Self::Nomap => "nomap",
            Self::Private => "private",
        }
    }
}

impl std::fmt::Display for UsernsMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Restart policy for a `container.services` entry. Serializes to the
/// same strings as before, so the services JSON the guest reads is
/// unchanged.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, strum::VariantNames,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum ServiceRestart {
    Never,
    #[default]
    OnFailure,
    Always,
}

impl ServiceRestart {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::OnFailure => "on-failure",
            Self::Always => "always",
        }
    }
}

impl std::fmt::Display for ServiceRestart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum GpuMode {
    #[default]
    Auto,
    Enabled,
    Disabled,
    Nvidia,
}

impl<'de> Deserialize<'de> for GpuMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;

        struct GpuModeVisitor;

        impl<'de> de::Visitor<'de> for GpuModeVisitor {
            type Value = GpuMode;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("true, false, \"auto\", or \"nvidia\"")
            }

            fn visit_bool<E: de::Error>(self, v: bool) -> Result<GpuMode, E> {
                Ok(if v {
                    GpuMode::Enabled
                } else {
                    GpuMode::Disabled
                })
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<GpuMode, E> {
                match v {
                    "auto" => Ok(GpuMode::Auto),
                    "nvidia" => Ok(GpuMode::Nvidia),
                    "true" => Ok(GpuMode::Enabled),
                    "false" => Ok(GpuMode::Disabled),
                    _ => Err(de::Error::unknown_variant(
                        v,
                        &["auto", "nvidia", "true", "false"],
                    )),
                }
            }
        }

        deserializer.deserialize_any(GpuModeVisitor)
    }
}

impl Serialize for GpuMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            GpuMode::Auto => serializer.serialize_str("auto"),
            GpuMode::Enabled => serializer.serialize_bool(true),
            GpuMode::Disabled => serializer.serialize_bool(false),
            GpuMode::Nvidia => serializer.serialize_str("nvidia"),
        }
    }
}

/// Predefined capability profiles that bundle commonly-needed Linux
/// capabilities.  The profile caps are always emitted; users can add
/// extra capabilities via `security.cap_add`.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, strum::VariantNames,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum CapPreset {
    /// No extra capabilities beyond Podman's defaults.
    None,
    /// Sane default: `DAC_READ_SEARCH`, `PERFMON` — needed by tools
    /// such as btop and htop for file-descriptor and perf monitoring.
    #[default]
    Default,
    /// Monitoring / observability: like `Default` plus `SYS_PTRACE`,
    /// `SYS_NICE`, `SYSLOG`.
    Monitoring,
    /// Powerful / dangerous: like `Monitoring` plus `SYS_ADMIN`,
    /// `NET_ADMIN`.  Only use when you trust the container fully.
    Admin,
}

impl CapPreset {
    pub fn caps(&self) -> &'static [&'static str] {
        match self {
            Self::None => &[],
            Self::Default => &["DAC_READ_SEARCH", "PERFMON"],
            Self::Monitoring => &["DAC_READ_SEARCH", "PERFMON", "SYS_PTRACE", "SYS_NICE"],
            Self::Admin => &[
                "DAC_READ_SEARCH",
                "PERFMON",
                "SYS_PTRACE",
                "SYS_NICE",
                "SYS_ADMIN",
                "NET_ADMIN",
                "SYSLOG",
            ],
        }
    }
}

#[derive(
    Debug, Default, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, strum::VariantNames,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum OnStop {
    #[default]
    Keep,
    Remove,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum XdgDirValue {
    Simple(bool),
    Detailed {
        enabled: bool,
        #[serde(default)]
        read_write: bool,
    },
}

impl Default for XdgDirValue {
    fn default() -> Self {
        XdgDirValue::Simple(false)
    }
}

impl XdgDirValue {
    pub fn is_enabled(&self) -> bool {
        match self {
            XdgDirValue::Simple(b) => *b,
            XdgDirValue::Detailed { enabled, .. } => *enabled,
        }
    }

    pub fn is_read_write(&self) -> bool {
        match self {
            XdgDirValue::Simple(_) => false,
            XdgDirValue::Detailed { read_write, .. } => *read_write,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    use super::*;

    #[test]
    fn test_gpu_mode_parses_true() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[integration]
gpu = true
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.integration.gpu, GpuMode::Enabled);
    }

    #[test]
    fn test_gpu_mode_parses_false() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[integration]
gpu = false
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.integration.gpu, GpuMode::Disabled);
    }

    #[test]
    fn test_gpu_mode_parses_auto_string() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[integration]
gpu = "auto"
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.integration.gpu, GpuMode::Auto);
    }

    #[test]
    fn test_gpu_mode_parses_nvidia_string() {
        let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[integration]
gpu = "nvidia"
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.integration.gpu, GpuMode::Nvidia);
    }

    #[test]
    fn test_gpu_mode_serialize() {
        assert_eq!(serde_json::to_string(&GpuMode::Auto).unwrap(), "\"auto\"");
        assert_eq!(serde_json::to_string(&GpuMode::Enabled).unwrap(), "true");
        assert_eq!(serde_json::to_string(&GpuMode::Disabled).unwrap(), "false");
        assert_eq!(
            serde_json::to_string(&GpuMode::Nvidia).unwrap(),
            "\"nvidia\""
        );
        #[derive(Serialize)]
        struct Wrapper {
            gpu: GpuMode,
        }
        let wrapper = Wrapper {
            gpu: GpuMode::Nvidia,
        };
        let toml_out = toml::to_string(&wrapper).unwrap();
        assert!(toml_out.contains("gpu = \"nvidia\""));
        let wrapper = Wrapper {
            gpu: GpuMode::Enabled,
        };
        let toml_out = toml::to_string(&wrapper).unwrap();
        assert!(toml_out.contains("gpu = true"));
    }

    #[test]
    fn test_gpu_mode_deserialize_toml_key() {
        let cases = [
            ("gpu = true", GpuMode::Enabled),
            ("gpu = false", GpuMode::Disabled),
            ("gpu = \"auto\"", GpuMode::Auto),
            ("gpu = \"nvidia\"", GpuMode::Nvidia),
        ];
        for (toml_snippet, expected) in &cases {
            let full = format!(
                r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[integration]
{toml_snippet}
"#
            );
            let cfg: Config = toml::from_str(&full).unwrap();
            assert_eq!(cfg.integration.gpu, *expected);
        }
    }

    fn config_with(tail: &str) -> String {
        format!(
            "[image]\nbase = \"fedora:41\"\nname = \"env\"\n\
             [container]\nname = \"env\"\nhome = \"~/env\"\n{tail}\n"
        )
    }

    #[test]
    fn test_network_mode_values_parse() {
        for (value, expected) in [
            ("host", NetworkMode::Host),
            ("bridge", NetworkMode::Bridge),
            ("none", NetworkMode::None),
            ("pasta", NetworkMode::Pasta),
            ("slirp4netns", NetworkMode::Slirp4netns),
            ("private", NetworkMode::Private),
        ] {
            let cfg: Config =
                toml::from_str(&config_with(&format!("[network]\nmode = \"{value}\"\n"))).unwrap();
            assert_eq!(cfg.network.mode, expected);
        }
    }

    #[test]
    fn test_network_mode_typo_lists_valid_values() {
        let err = toml::from_str::<Config>(&config_with("[network]\nmode = \"macvlan\"\n"))
            .expect_err("typoed mode must fail");
        let msg = err.to_string();
        for valid in ["host", "bridge", "none", "pasta", "slirp4netns", "private"] {
            assert!(msg.contains(valid), "error should list {valid}: {msg}");
        }
    }

    #[test]
    fn test_userns_values_parse() {
        for (value, expected) in [
            ("keep-id", UsernsMode::KeepId),
            ("nomap", UsernsMode::Nomap),
            ("private", UsernsMode::Private),
        ] {
            let cfg: Config =
                toml::from_str(&config_with(&format!("[security]\nuserns = \"{value}\"\n")))
                    .unwrap();
            assert_eq!(cfg.security.userns, Some(expected));
        }
    }

    #[test]
    fn test_userns_typo_lists_valid_values() {
        let err = toml::from_str::<Config>(&config_with("[security]\nuserns = \"keepid\"\n"))
            .expect_err("typoed userns must fail");
        let msg = err.to_string();
        for valid in ["keep-id", "nomap", "private"] {
            assert!(msg.contains(valid), "error should list {valid}: {msg}");
        }
    }

    #[test]
    fn test_service_restart_values_parse() {
        for (value, expected) in [
            ("never", ServiceRestart::Never),
            ("on-failure", ServiceRestart::OnFailure),
            ("always", ServiceRestart::Always),
        ] {
            let cfg: Config = toml::from_str(&config_with(&format!(
                "[container.services.web]\ncommand = \"x\"\nrestart = \"{value}\"\n"
            )))
            .unwrap();
            match &cfg.container.services["web"] {
                crate::config::ServiceConfig::Detailed(detail) => {
                    assert_eq!(detail.restart, expected);
                }
                other => panic!("expected detailed service, got {other:?}"),
            }
        }
    }

    #[test]
    fn test_service_restart_typo_lists_valid_values() {
        let err = toml::from_str::<Config>(&config_with(
            "[container.services.web]\ncommand = \"x\"\nrestart = \"sometimes\"\n",
        ))
        .expect_err("typoed restart must fail");
        let msg = err.to_string();
        for valid in ["never", "on-failure", "always"] {
            assert!(msg.contains(valid), "error should list {valid}: {msg}");
        }
    }

    #[test]
    fn test_dbus_preset_typo_lists_valid_values() {
        let err = toml::from_str::<Config>(&config_with("[dbus]\npreset = \"flatpack\"\n"))
            .expect_err("typoed preset must fail");
        let msg = err.to_string();
        for valid in ["flatpak", "gnome", "kde", "portal", "none"] {
            assert!(msg.contains(valid), "error should list {valid}: {msg}");
        }
    }
}
