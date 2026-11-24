use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::defaults::is_false;

#[derive(
    Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq, strum::VariantNames,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum DotfilesCloneOn {
    #[default]
    Host,
    Container,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DotfilesConfig {
    pub source: String,
    #[serde(
        default = "default_dotfiles_target",
        skip_serializing_if = "is_default_dotfiles_target"
    )]
    pub target: String,
    #[serde(default, skip_serializing_if = "is_default_dotfiles_clone_on")]
    pub clone_on: DotfilesCloneOn,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<String>,
}

/// Canonical `dotfiles.target` default; shared with the schema-defaults
/// table so it cannot drift there.
pub const DOTFILES_TARGET: &str = "~/.dotfiles";

fn default_dotfiles_target() -> String {
    DOTFILES_TARGET.into()
}

fn is_default_dotfiles_target(v: &str) -> bool {
    v == DOTFILES_TARGET
}

fn is_default_dotfiles_clone_on(v: &DotfilesCloneOn) -> bool {
    *v == DotfilesCloneOn::default()
}

fn default_provision_timeout() -> String {
    "5m".into()
}

#[derive(
    Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq, strum::VariantNames,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ProvisionOnFailure {
    #[default]
    Warn,
    Abort,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
pub struct ProvisionStep {
    pub name: String,
    pub run: String,
    /// Postcondition, run after `run` exits 0: it must exit 0 too or the
    /// step counts as failed. Catches tools that report partial failure
    /// with a success exit code — `android sdk install platform-tools`
    /// prints "package not found" and still exits 0 — by asking the
    /// system what state it is actually in.
    ///
    /// Hashed with the step (like `run`), so editing the check re-runs it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<String>,
    #[serde(
        default = "default_provision_timeout",
        skip_serializing_if = "is_default_provision_timeout"
    )]
    pub timeout: String,
    #[serde(default, skip_serializing_if = "is_default_provision_on_failure")]
    pub on_failure: ProvisionOnFailure,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub root: bool,
}

fn is_default_provision_timeout(v: &str) -> bool {
    v == default_provision_timeout()
}

fn is_default_provision_on_failure(v: &ProvisionOnFailure) -> bool {
    *v == ProvisionOnFailure::Warn
}
