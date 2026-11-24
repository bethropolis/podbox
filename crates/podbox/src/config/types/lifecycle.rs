use serde::{Deserialize, Serialize};

use crate::config::defaults::is_false;
use crate::config::enums::OnStop;

fn is_default_on_stop(on_stop: &OnStop) -> bool {
    *on_stop == OnStop::Keep
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LifecycleConfig {
    #[serde(default, skip_serializing_if = "is_false")]
    pub quadlet: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub autostart: bool,
    #[serde(default, skip_serializing_if = "is_default_on_stop")]
    pub on_stop: OnStop,
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_update: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_checkpoint: bool,
    #[serde(
        default = "crate::config::defaults::default_idle_timeout",
        skip_serializing_if = "crate::config::defaults::is_default_idle_timeout"
    )]
    pub idle_timeout: String,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        LifecycleConfig {
            quadlet: false,
            autostart: false,
            on_stop: OnStop::Keep,
            auto_update: false,
            auto_checkpoint: false,
            idle_timeout: crate::config::defaults::default_idle_timeout(),
        }
    }
}

impl LifecycleConfig {
    /// True when the section is exactly what an absent `[lifecycle]` table
    /// means; written TOML then omits the header entirely.
    pub fn is_default(&self) -> bool {
        !self.quadlet
            && !self.autostart
            && is_default_on_stop(&self.on_stop)
            && !self.auto_update
            && !self.auto_checkpoint
            && crate::config::defaults::is_default_idle_timeout(&self.idle_timeout)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct SystemdConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
}

impl SystemdConfig {
    /// True when the section is exactly what an absent `[systemd]` table
    /// means; written TOML then omits the header entirely.
    pub fn is_default(&self) -> bool {
        self.requires.is_empty() && self.after.is_empty()
    }
}
