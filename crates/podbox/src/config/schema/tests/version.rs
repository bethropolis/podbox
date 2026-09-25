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
