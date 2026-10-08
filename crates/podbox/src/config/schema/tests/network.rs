//! Network mode and published port validation

use super::super::*;

#[test]
fn test_network_defaults_to_pasta() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert_eq!(cfg.network.mode, "pasta");
    assert!(!cfg.network.offline);
    assert!(cfg.network.ports.is_empty());
}

#[test]
fn offline_forces_quadlet_network_none_without_mutating_mode() {
    let cfg = Config::parse(
        r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[network]
mode = "pasta"
offline = true
"#,
    )
    .unwrap();
    assert_eq!(cfg.network.mode, "pasta");
    assert!(cfg.network.offline);
    assert_eq!(cfg.network.effective_mode(), "none");
    let serialized = toml::to_string(&cfg).unwrap();
    assert!(serialized.contains("offline = true"));
}
#[test]
fn test_network_parses_mode_and_ports() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[network]
mode = "pasta"
ports = ["8080:80", "443:443"]
"#;
    let cfg = Config::parse(toml).unwrap();
    assert_eq!(cfg.network.mode, "pasta");
    assert_eq!(cfg.network.ports, vec!["8080:80", "443:443"]);
}
#[test]
fn test_network_invalid_mode_rejected() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[network]
mode = "macvlan"
"#;
    assert!(Config::parse(toml).is_err());
}
#[test]
fn test_network_port_missing_separator_rejected() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[network]
mode = "bridge"
ports = ["8080"]
"#;
    assert!(Config::parse(toml).is_err());
}
