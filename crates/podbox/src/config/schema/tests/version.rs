//! Schema version defaults and migration chain

use super::super::*;

#[test]
fn test_schema_version_defaults_to_current() {
    let cfg = Config::embedded();
    assert_eq!(cfg.schema_version.as_u32(), CURRENT_SCHEMA_VERSION);
}
#[test]
fn test_schema_version_parsed_from_toml() {
    let toml = r#"
schema_version = 1
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert_eq!(cfg.schema_version.as_u32(), 1);
}
#[test]
fn test_schema_version_defaults_when_omitted() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert_eq!(cfg.schema_version.as_u32(), CURRENT_SCHEMA_VERSION);
}
#[test]
fn test_schema_version_migration_bumps_old_schema() {
    let toml = r#"
schema_version = 0
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert_eq!(cfg.schema_version.as_u32(), CURRENT_SCHEMA_VERSION);
}
#[test]
fn test_schema_version_from_the_future_is_rejected_with_upgrade_hint() {
    let toml = r#"
schema_version = 2
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
    let err = Config::parse(toml).expect_err("a future schema_version must fail");
    let msg = err.to_string();
    assert!(msg.contains("schema 2"), "{msg}");
    assert!(msg.contains("upgrade podbox"), "{msg}");
}
#[test]
fn test_unknown_top_level_key_is_rejected() {
    let toml = r#"
netwrok = "host"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
    let err = Config::parse(toml).expect_err("a typoed top-level key must fail");
    assert!(format!("{err:#}").contains("netwrok"), "{err:#}");
}
