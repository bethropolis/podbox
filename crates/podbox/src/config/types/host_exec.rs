use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::defaults::default_true;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HostExecEntry {
    /// Simple path string (defaults: `filter = false`, `shim = true`).
    Simple(String),
    /// Detailed configuration table.
    Detailed {
        path: String,
        /// Best-effort argument blocklist. Off by default: the allowlist
        /// entry is already a trusted program, and the blocklist rejects
        /// harmless globs/metacharacters. Opt in for extra guardrails.
        #[serde(default)]
        filter: bool,
        #[serde(default = "default_true")]
        shim: bool,
        /// Kill the child after this many seconds and report the death as
        /// `128 + signal` on the fd-passing session. Omitted means no
        /// timeout; TTY sessions are never time-limited by it (see the
        /// `timeout_secs` docs in `docs/config.md`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout: Option<u32>,
    },
}

impl HostExecEntry {
    pub fn path(&self) -> &str {
        match self {
            Self::Simple(p) => p.as_str(),
            Self::Detailed { path, .. } => path.as_str(),
        }
    }

    pub fn filter_enabled(&self) -> bool {
        match self {
            Self::Simple(_) => false,
            Self::Detailed { filter, .. } => *filter,
        }
    }

    pub fn shim_enabled(&self) -> bool {
        match self {
            Self::Simple(_) => true,
            Self::Detailed { shim, .. } => *shim,
        }
    }

    /// The entry's optional execution timeout, in seconds. `None` means the
    /// child may run as long as it keeps producing output; per-entry
    /// timeouts are opt-in because an allowlisted command's natural runtime
    /// is the administrator's knowledge, not the engine's.
    pub fn timeout_secs(&self) -> Option<u64> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed { timeout, .. } => timeout.map(u64::from),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostExecConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowlist: Option<BTreeMap<String, HostExecEntry>>,
    /// How the fd-passing session maps the guest's working directory onto
    /// the host. Runtime behaviour, not codegen: the daemon consults this
    /// per request, so no generated unit carries it.
    #[serde(default, skip_serializing_if = "is_default_host_exec_cwd")]
    pub cwd: HostExecCwd,
}

/// What the host does with the `cwd` a fd-passing request carries.
#[derive(
    Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq, strum::VariantNames,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum HostExecCwd {
    /// Translate the guest path through the container's known bind
    /// mappings (home volume, XDG dirs, host caches, extra mounts). A path
    /// with no mapping falls back to the host home with a stderr warning
    /// the guest sees.
    #[default]
    Map,
    /// Ignore the guest cwd entirely: the child starts in the host home.
    Home,
    /// Refuse the request with a typed error naming the guest path.
    Refuse,
}

/// The `skip_serializing_if` hook for `integration.host_exec.cwd`: the
/// default (`map`) stays out of written TOML like every untouched key.
fn is_default_host_exec_cwd(v: &HostExecCwd) -> bool {
    *v == HostExecCwd::default()
}

impl HostExecConfig {
    pub fn resolve(&self, cmd: &str) -> Option<&HostExecEntry> {
        if !self.enabled {
            return None;
        }
        match &self.allowlist {
            Some(map) => map.get(cmd),
            None => None,
        }
    }

    /// List of alias names that should have PATH shims generated in the guest.
    pub fn guest_shims(&self) -> Vec<String> {
        if !self.enabled {
            return Vec::new();
        }
        self.allowlist
            .as_ref()
            .map(|map| {
                map.iter()
                    .filter(|(_, entry)| entry.shim_enabled())
                    .map(|(alias, _)| alias.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}
