use serde::{Deserialize, Serialize};

use crate::config::defaults::{default_network_mode, is_false};
use crate::config::enums::{CapPreset, NetworkMode, UsernsMode};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NetworkConfig {
    #[serde(
        default = "default_network_mode",
        skip_serializing_if = "is_default_network_mode"
    )]
    pub mode: NetworkMode,
    #[serde(default, skip_serializing_if = "is_false")]
    pub offline: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mode: default_network_mode(),
            offline: false,
            ports: Vec::new(),
        }
    }
}

fn is_default_network_mode(mode: &NetworkMode) -> bool {
    *mode == default_network_mode()
}

impl NetworkConfig {
    /// True when the section is exactly what an absent `[network]` table
    /// means; written TOML then omits the header entirely.
    pub fn is_default(&self) -> bool {
        is_default_network_mode(&self.mode) && !self.offline && self.ports.is_empty()
    }
    pub fn effective_mode(&self) -> NetworkMode {
        if self.offline {
            NetworkMode::None
        } else {
            self.mode
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum SecretEntry {
    Simple(String),
    Detailed {
        name: String,
        #[serde(default = "default_secret_type")]
        #[serde(rename = "type")]
        secret_type: SecretType,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(default = "default_secret_source")]
        source: SecretSource,
        /// Host-side credential file for `source = "systemd"`, as passed to
        /// `LoadCredential=` / `LoadCredentialEncrypted=`. Required then.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        /// Read `path` with `LoadCredentialEncrypted=` instead of
        /// `LoadCredential=`. Only meaningful with `source = "systemd"`.
        #[serde(default, skip_serializing_if = "is_false")]
        encrypted: bool,
    },
}

fn default_secret_type() -> SecretType {
    SecretType::Env
}
fn default_secret_source() -> SecretSource {
    SecretSource::Podman
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, strum::VariantNames,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum SecretType {
    #[default]
    Env,
    Mount,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, strum::VariantNames,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum SecretSource {
    #[default]
    Podman,
    Systemd,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SecurityConfig {
    #[serde(default)]
    pub apparmor: Option<String>,
    #[serde(default)]
    pub seccomp: Option<String>,
    #[serde(
        default = "crate::config::defaults::default_true",
        skip_serializing_if = "crate::config::defaults::is_true"
    )]
    pub security_label_disable: bool,
    #[serde(
        default = "crate::config::defaults::default_true",
        skip_serializing_if = "crate::config::defaults::is_true"
    )]
    pub no_new_privileges: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub read_only_rootfs: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub userns: Option<UsernsMode>,
    #[serde(default, skip_serializing_if = "is_default_cap_preset")]
    pub cap_preset: CapPreset,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cap_add: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<SecretEntry>,
}

impl SecurityConfig {
    /// True when the section is exactly what an absent `[security]` table
    /// means; written TOML then omits the header entirely.
    pub fn is_default(&self) -> bool {
        self.apparmor.is_none()
            && self.seccomp.is_none()
            && self.security_label_disable
            && self.no_new_privileges
            && !self.read_only_rootfs
            && self.userns.is_none()
            && is_default_cap_preset(&self.cap_preset)
            && self.cap_add.is_empty()
            && self.secrets.is_empty()
    }
}

fn is_default_cap_preset(v: &CapPreset) -> bool {
    *v == CapPreset::Default
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            apparmor: None,
            seccomp: None,
            security_label_disable: true,
            no_new_privileges: true,
            read_only_rootfs: false,
            userns: None,
            cap_preset: CapPreset::Default,
            cap_add: Vec::new(),
            secrets: Vec::new(),
        }
    }
}
