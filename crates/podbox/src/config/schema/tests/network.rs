//! Network mode and published port validation

use super::super::*;

#[test]
fn test_network_defaults_to_private() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert_eq!(cfg.network.mode, "private");
    assert!(cfg.network.ports.is_empty());
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
