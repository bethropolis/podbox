use serde::{Deserialize, Serialize};

use crate::config::defaults::{default_network_mode, is_false};
use crate::config::enums::CapPreset;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NetworkConfig {
    #[serde(default = "default_network_mode")]
    pub mode: String,
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

impl NetworkConfig {
    pub fn effective_mode(&self) -> &str {
        if self.offline { "none" } else { &self.mode }
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
    },
}

fn default_secret_type() -> SecretType {
    SecretType::Env
}
fn default_secret_source() -> SecretSource {
    SecretSource::Podman
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SecretType {
    #[default]
    Env,
    Mount,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
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
    pub userns: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_cap_preset")]
    pub cap_preset: CapPreset,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cap_add: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<SecretEntry>,
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
