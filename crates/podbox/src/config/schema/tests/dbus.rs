//! D-Bus talk list and portal rule derivation

use super::super::*;

#[test]
fn test_dbus_config_defaults_empty() {
    // Desktop capabilities are opt-in: the embedded default enables none of
    // them, so no proxy units or portal rules are generated.
    let cfg = Config::embedded();
    assert_eq!(cfg.dbus.preset, "portal");
    assert!(cfg.dbus_effective_talk().is_empty());
    assert!(cfg.dbus_portal_calls().is_empty());
    assert!(!cfg.use_dbus_proxy());
}
#[test]
fn test_dbus_portal_dropped_when_caps_disabled() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
preset = "portal"
[integration]
notify = false
xdg_open = false
clipboard = false
"#;
    let cfg = Config::parse(toml).unwrap();
    assert!(cfg.dbus_effective_talk().is_empty());
    assert!(cfg.dbus_portal_calls().is_empty());
    assert!(!cfg.use_dbus_proxy());
}
#[test]
fn test_dbus_portal_kept_when_notify_enabled() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
preset = "portal"
[integration]
notify = true
xdg_open = false
clipboard = false
"#;
    let cfg = Config::parse(toml).unwrap();
    assert!(cfg.dbus_effective_talk().is_empty());
    assert!(cfg.use_dbus_proxy());
    let calls = cfg.dbus_portal_calls();
    assert_eq!(calls.len(), 4);
    assert!(calls[0].starts_with("--call=org.freedesktop.portal.Desktop="));
    assert!(calls[0].contains("org.freedesktop.portal.Notification.*"));
    assert!(calls[1].starts_with("--call=org.freedesktop.portal.Desktop="));
    assert!(calls[1].contains("org.freedesktop.portal.Request.*"));
    assert!(calls[2].starts_with("--broadcast=org.freedesktop.portal.Desktop="));
    assert!(calls[2].contains("org.freedesktop.portal.Request.*"));
    assert!(calls[3].starts_with("--call=org.freedesktop.portal.Desktop="));
    assert!(calls[3].contains("org.freedesktop.DBus.Introspectable.*"));
}
#[test]
fn test_dbus_portal_calls_gated_by_capabilities() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
preset = "portal"
[integration]
notify = false
xdg_open = true
clipboard = false
"#;
    let cfg = Config::parse(toml).unwrap();
    let calls = cfg.dbus_portal_calls();
    assert_eq!(calls.len(), 4);
    assert!(!calls.iter().any(|r| r.contains("Notification")));
    assert!(calls.iter().any(|r| r.contains("OpenURI.*")));
    assert!(calls.iter().any(|r| r.contains("Introspectable")));
}
#[test]
fn test_dbus_config_parses_talk_own() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
talk = ["org.freedesktop.Notifications", "org.mpris.MediaPlayer2.*"]
own = ["org.mpris.MediaPlayer2.podbox_app"]
"#;
    let cfg = Config::parse(toml).unwrap();
    assert_eq!(
        cfg.dbus.talk,
        vec!["org.freedesktop.Notifications", "org.mpris.MediaPlayer2.*"]
    );
    assert_eq!(cfg.dbus.own, vec!["org.mpris.MediaPlayer2.podbox_app"]);
    assert!(cfg.use_dbus_proxy());
}
#[test]
fn test_dbus_config_talk_only() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
talk = ["org.freedesktop.Notifications"]
"#;
    let cfg = Config::parse(toml).unwrap();
    assert_eq!(cfg.dbus.talk.len(), 1);
    assert!(cfg.dbus.own.is_empty());
    assert!(cfg.use_dbus_proxy());
}
#[test]
fn test_dbus_config_own_only() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
own = ["org.example.Service"]
"#;
    let cfg = Config::parse(toml).unwrap();
    assert!(cfg.dbus.talk.is_empty());
    assert_eq!(cfg.dbus.own.len(), 1);
    assert!(cfg.use_dbus_proxy());
}
