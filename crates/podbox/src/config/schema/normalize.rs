use anyhow::{Context, Result};

use crate::config::DbusPreset;

use super::{CURRENT_SCHEMA_VERSION, Config};

impl Config {
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

    /// Deserialize, migrate and default a merged TOML document without
    /// validating. The source-aware and lenient loaders share this so both
    /// see identical migrations and defaults.
    pub(super) fn from_merged(value: toml::Value) -> Result<Config> {
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
