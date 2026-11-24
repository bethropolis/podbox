use serde::{Deserialize, Serialize};

use crate::config::defaults::{default_network_mode, is_false};
use crate::config::enums::NetworkMode;

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
