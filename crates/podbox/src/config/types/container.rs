use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::defaults::{default_shell, is_default_mounts, is_default_shell};
use crate::config::enums::ServiceRestart;
use crate::config::expand_tilde;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContainerConfig {
    pub name: String,
    #[serde(deserialize_with = "deserialize_home")]
    pub home: PathBuf,
    #[serde(default = "default_shell", skip_serializing_if = "is_default_shell")]
    pub shell: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpus: Option<String>,
    /// systemd slice for this container's service cgroup.
    #[serde(
        default = "default_container_slice",
        skip_serializing_if = "is_default_container_slice"
    )]
    pub slice: String,
    /// systemd CPU scheduling weight (1..=10000).
    #[serde(
        default = "default_cpu_weight",
        skip_serializing_if = "is_default_cpu_weight"
    )]
    pub cpu_weight: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reload_cmd: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_mounts")]
    pub mounts: MountConfig,
    #[serde(default, skip_serializing_if = "ContainerEnvConfig::is_empty")]
    pub env: ContainerEnvConfig,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub services: BTreeMap<String, ServiceConfig>,
}

// Untagged so the short form serializes as a plain `name = "command"`
// string and the detailed form as a table — exactly what the custom
// reader accepts, and what the original (externally tagged) output
// could not express in TOML.
#[derive(Debug, Serialize, Clone)]
#[serde(untagged)]
pub enum ServiceConfig {
    Short(String),
    Detailed(ServiceDetailConfig),
}

// Custom deserialization so a typo inside the detailed table (for
// example `restart = "sometimes"`) surfaces the table error — which
// lists the valid values — instead of the opaque "data did not match
// any variant of untagged enum" that `#[serde(untagged)]` produces.
impl<'de> Deserialize<'de> for ServiceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = toml::Value::deserialize(deserializer)?;
        match value {
            toml::Value::String(command) => Ok(Self::Short(command)),
            toml::Value::Table(_) => value
                .try_into::<ServiceDetailConfig>()
                .map(Self::Detailed)
                .map_err(serde::de::Error::custom),
            other => Err(serde::de::Error::custom(format!(
                "expected a command string or a service table, got {}",
                other.type_str()
            ))),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServiceDetailConfig {
    pub command: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_default_restart")]
    pub restart: ServiceRestart,
}

fn is_default_restart(restart: &ServiceRestart) -> bool {
    *restart == ServiceRestart::default()
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ContainerEnvConfig {
    #[serde(flatten)]
    pub values: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forward: Vec<String>,
}

/// Resolve a `container.shell` value to the binary path terminals expect in
/// `$SHELL`. Accepts full paths verbatim; well-known names map to their
/// distro-standard locations. Anything else yields `None` (no derivation).
fn shell_binary(shell: &str) -> Option<String> {
    if shell.contains('/') {
        return Some(shell.to_string());
    }
    match shell {
        "bash" => Some("/bin/bash".into()),
        "fish" => Some("/usr/bin/fish".into()),
        "zsh" => Some("/bin/zsh".into()),
        "sh" => Some("/bin/sh".into()),
        _ => None,
    }
}

impl ContainerConfig {
    /// Effective container environment: explicit `[container.env]` values
    /// plus engine-derived defaults for well-known variables the config
    /// already implies. Explicit values always win.
    ///
    /// Current derivations (first of a general mechanism — add more here as
    /// new "config already says it" variables emerge):
    /// - `SHELL` from `container.shell`, so terminals, `enter`, and GUI
    ///   apps agree without repeating the path in every TOML.
    pub fn effective_env(&self) -> BTreeMap<String, String> {
        let mut out = self.env.values.clone();
        if !out.contains_key("SHELL") {
            if let Some(path) = shell_binary(&self.shell) {
                out.insert("SHELL".into(), path);
            }
        }
        out
    }
}

impl ContainerEnvConfig {
    fn is_empty(&self) -> bool {
        self.values.is_empty() && self.forward.is_empty()
    }
}

impl std::ops::Deref for ContainerEnvConfig {
    type Target = BTreeMap<String, String>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

fn default_container_slice() -> String {
    "podbox.slice".into()
}
fn is_default_container_slice(value: &String) -> bool {
    value == "podbox.slice"
}
fn default_cpu_weight() -> u16 {
    200
}
fn is_default_cpu_weight(value: &u16) -> bool {
    *value == 200
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct MountConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<String>,
}

fn deserialize_home<'de, D>(deserializer: D) -> std::result::Result<PathBuf, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let path = String::deserialize(deserializer)?;
    Ok(expand_tilde(&path))
}
