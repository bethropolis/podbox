//! Security profile defaults and userns modes

use super::super::*;

#[test]
fn test_security_read_only_rootfs_defaults_false() {
    let cfg = Config::embedded();
    assert!(!cfg.security.read_only_rootfs);
}
#[test]
fn test_security_userns_defaults_none() {
    let cfg = Config::embedded();
    assert!(cfg.security.userns.is_none());
}
#[test]
fn test_security_userns_valid_modes() {
    for mode in &["keep-id", "nomap", "private"] {
        let toml = format!(
            r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[security]
userns = "{mode}"
"#
        );
        assert!(
            Config::parse(&toml).is_ok(),
            "userns mode '{mode}' should be valid"
        );
    }
}
#[test]
fn test_security_userns_invalid_mode_rejected() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[security]
userns = "invalid"
"#;
    assert!(Config::parse(toml).is_err());
}
