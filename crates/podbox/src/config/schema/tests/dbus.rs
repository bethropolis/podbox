//! D-Bus talk list and portal rule derivation

use super::super::*;

#[test]
fn test_dbus_config_defaults_empty() {
    // D-Bus itself is opt-in: the embedded default enables nothing, so no
    // preset is applied and no proxy units or portal rules are generated.
    let cfg = Config::embedded();
    assert!(!cfg.integration.dbus);
    assert!(cfg.dbus.preset.is_empty());
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
dbus = true
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
dbus = true
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
dbus = true
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
[integration]
dbus = true
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
[integration]
dbus = true
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
[integration]
dbus = true
"#;
    let cfg = Config::parse(toml).unwrap();
    assert!(cfg.dbus.talk.is_empty());
    assert_eq!(cfg.dbus.own.len(), 1);
    assert!(cfg.use_dbus_proxy());
}

#[test]
fn dbus_rules_without_dbus_enabled_are_rejected() {
    // Talk rules with the bus off would be silently ignored — fail fast.
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
    let err = Config::parse(toml).unwrap_err().to_string();
    assert!(err.contains("integration.dbus"), "{err}");

    // ...including a preset without the bus.
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
preset = "portal"
"#;
    let err = Config::parse(toml).unwrap_err().to_string();
    assert!(err.contains("integration.dbus"), "{err}");

    // Explicit `preset = "none"` is a no-op, not dead config — allowed.
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[dbus]
preset = "none"
"#;
    Config::parse(toml).unwrap();
}
