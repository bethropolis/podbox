//! Resource limits: memory and CPU quota

use super::super::*;

#[test]
fn test_memory_decimal_rejected() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
memory = "1.5g"
"#;
    let cfg = Config::parse(toml);
    assert!(cfg.is_err(), "decimal memory should be rejected: {cfg:?}");
}
#[test]
fn test_memory_integer_accepted() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
memory = "2g"
"#;
    assert!(Config::parse(toml).is_ok());
}
#[test]
fn test_memory_bare_digits_rejected() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
memory = "2"
"#;
    let cfg = Config::parse(toml);
    assert!(
        cfg.is_err(),
        "bare memory without unit should be rejected: {cfg:?}"
    );
    let err = format!("{cfg:?}");
    assert!(err.contains("container.memory"));
}
#[test]
fn test_memory_bare_digits_helper() {
    use crate::config::validation::{is_bare_memory_digits, is_valid_memory};
    assert!(is_bare_memory_digits("2"));
    assert!(is_bare_memory_digits("  512  "));
    assert!(!is_bare_memory_digits("2g"));
    assert!(!is_valid_memory("2"));
    assert!(is_valid_memory("2G"));
    assert!(is_valid_memory("512m"));
}
#[test]
fn test_cpus_parses_valid() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
cpus = "2.0"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert_eq!(cfg.container.cpus.as_deref(), Some("2.0"));
}
#[test]
fn test_cpus_rejects_non_positive() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
cpus = "0"
"#;
    assert!(Config::parse(toml).is_err());
}
#[test]
fn test_cpus_defaults_to_none() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert!(cfg.container.cpus.is_none());
    assert_eq!(cfg.container.slice, "podbox.slice");
    assert_eq!(cfg.container.cpu_weight, 200);
}

#[test]
fn scheduling_weight_range_is_validated() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
cpu_weight = 0
"#;
    assert!(
        Config::parse(toml)
            .unwrap_err()
            .to_string()
            .contains("container.cpu_weight")
    );
}
