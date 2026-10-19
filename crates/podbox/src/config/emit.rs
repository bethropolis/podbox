//! Canonical TOML emission for `Config`.
//!
//! One place both `podbox init` and the wasm Studio bindings serialize a
//! resolved config through: `skip_serializing_if` on defaults keeps the
//! written file small, so callers emit without re-implementing
//! serialization by hand (and the Studio output matches the CLI's).
//!
//! Pure module: no host dependencies, compiles for wasm.

use anyhow::Result;

use super::Config;

/// Serialize a config as pretty TOML.
///
/// Deterministic aside from the values themselves: field order is the
/// struct order, and defaults are skipped, so a config that came from a
/// profile or wizard lands in the same shape readers expect.
pub fn toml_string(config: &Config) -> Result<String> {
    Ok(toml::to_string_pretty(config)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::enums::ServiceRestart;

    #[test]
    fn emits_untagged_short_form() {
        // The obsolete externally-tagged enum serialization wrote an
        // invalid `Short = "cmd"` key; the untagged form writes what
        // readers accept.
        let cfg = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "env"

[container]
name = "env"
home = "~/env"
services.web = "redis-server"
"#,
        )
        .unwrap();
        let out = toml_string(&cfg).unwrap();
        assert!(out.contains("web = \"redis-server\""), "{out}");
        assert!(!out.contains("Short"), "{out}");
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.container.services.len(), 1);
    }

    #[test]
    fn emits_detailed_service_without_empty_fields() {
        // `env` is empty and stays out; no empty `[container.services.web.env]`
        // table is written.
        let cfg = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "env"

[container]
name = "env"
home = "~/env"

[container.services.web]
command = "redis-server"
restart = "always"
"#,
        )
        .unwrap();
        let out = toml_string(&cfg).unwrap();
        assert!(out.contains("[container.services.web]"), "{out}");
        assert!(out.contains("command = \"redis-server\""), "{out}");
        assert!(!out.contains("[container.services.web.env]"), "{out}");
        let reparsed = Config::parse(&out).unwrap();
        match &reparsed.container.services["web"] {
            crate::config::ServiceConfig::Detailed(detail) => {
                assert_eq!(detail.command, "redis-server");
                assert_eq!(detail.restart, ServiceRestart::Always);
            }
            other => panic!("expected detailed service, got {other:?}"),
        }
    }
}
