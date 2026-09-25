//! Quadlet companion units: socket, build, D-Bus proxy, host service
//! Split out of the former monolithic `codegen_tests.rs`.

mod common;

use common::*;

use podbox::codegen::quadlet;
use std::path::PathBuf;

#[test]
fn quadlet_dbus_proxy_when_configured() {
    let mut config = load_config("full.toml");
    config.dbus.talk = vec!["org.freedesktop.Notifications".into()];
    config.dbus.own = vec!["org.mpris.MediaPlayer2.podbox_app".into()];
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    // Proxy socket always, never unfiltered %t/bus
    assert!(q.contains("Volume=%t/podbox/myenv-dbus.sock:/run/podbox/dbus.sock:ro"));
    assert!(q.contains("Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/podbox/dbus.sock"));
    assert!(!q.contains("Volume=%t/bus:%t/bus"));
}
#[test]
fn quadlet_dbus_proxy_deps_in_unit() {
    let mut config = load_config("full.toml");
    config.dbus.talk = vec!["org.freedesktop.Notifications".into()];
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Requires=myenv-proxy.service"));
    assert!(q.contains("After=myenv-proxy.service"));
}
#[test]
fn quadlet_dbus_proxy_unit_generated() {
    let mut config = load_config("full.toml");
    config.dbus.talk = vec!["org.freedesktop.Notifications".into()];
    config.dbus.own = vec!["org.mpris.MediaPlayer2.podbox_app".into()];
    let unit = quadlet::generate_dbus_proxy_service("myenv", &config)
        .expect("proxy service should be generated");
    assert!(unit.contains("[Unit]"));
    assert!(unit.contains("Description=D-Bus Proxy for podbox container myenv"));
    assert!(unit.contains("PartOf=myenv.service"));
    assert!(unit.contains("[Service]"));
    assert!(!unit.contains("RuntimeDirectory=podbox"));
    assert!(unit.contains("/usr/bin/xdg-dbus-proxy"));
    assert!(unit.contains("--filter"));
    assert!(unit.contains("--talk=org.freedesktop.Notifications"));
    assert!(unit.contains("--own=org.mpris.MediaPlayer2.podbox_app"));
    assert!(unit.contains("%t/podbox/myenv-dbus.sock"));
    assert!(unit.contains("[Install]"));
    assert!(unit.contains("WantedBy=myenv.service"));
}
#[test]
fn quadlet_dbus_proxy_portal_rules_interface_scoped() {
    let mut config = load_config("full.toml");
    config.dbus.talk = vec!["org.freedesktop.Notifications".into()];
    let unit = quadlet::generate_dbus_proxy_service("myenv", &config)
        .expect("proxy service should be generated");
    assert!(
        !unit.contains("--talk=org.freedesktop.portal.*"),
        "portal name must never be granted wholesale via --talk"
    );
    assert!(unit.contains(
        "--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.Notification.*@/org/freedesktop/portal/desktop"
    ));
    assert!(unit.contains(
        "--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.OpenURI.*@/org/freedesktop/portal/desktop"
    ));
    assert!(unit.contains(
        "--call=org.freedesktop.portal.Desktop=org.freedesktop.portal.Request.*@/org/freedesktop/portal/desktop/request/*"
    ));
    assert!(unit.contains(
        "--broadcast=org.freedesktop.portal.Desktop=org.freedesktop.portal.Request.*@/org/freedesktop/portal/desktop/request/*"
    ));
    assert!(unit.contains(
        "--call=org.freedesktop.portal.Desktop=org.freedesktop.DBus.Introspectable.*@/org/freedesktop/portal/*"
    ));
    assert!(!unit.contains("DynamicLauncher"));
}
#[test]
fn quadlet_dbus_proxy_portal_rules_absent_when_caps_disabled() {
    let mut config = load_config("full.toml");
    config.dbus.talk = vec!["org.freedesktop.Notifications".into()];
    config.integration.notify = false;
    config.integration.xdg_open = false;
    config.integration.clipboard = false;
    let unit = quadlet::generate_dbus_proxy_service("myenv", &config)
        .expect("proxy service should be generated");
    assert!(!unit.contains("org.freedesktop.portal"));
}
#[test]
fn quadlet_dbus_proxy_unit_none_when_dbus_disabled() {
    let mut config = load_config("full.toml");
    config.integration.dbus = false;
    assert!(quadlet::generate_dbus_proxy_service("myenv", &config).is_none());
}
#[test]
fn quadlet_dbus_proxy_unit_generated_when_dbus_enabled() {
    let mut config = load_config("full.toml");
    config.dbus.talk = vec!["org.freedesktop.Notifications".into()];
    let unit = quadlet::generate_dbus_proxy_service("myenv", &config)
        .expect("proxy service should be generated when dbus enabled");
    assert!(unit.contains("--filter"));
    assert!(unit.contains("--talk="));
    assert!(!unit.contains("--own="));
}
#[test]
fn quadlet_host_service_has_restart_sec() {
    let unit = quadlet::generate_host_service("myenv");
    assert!(unit.contains("Restart=on-failure"));
    assert!(unit.contains("RestartSec=2s"));
}
#[test]
fn quadlet_dbus_proxy_unit_has_restart_sec() {
    let mut config = load_config("full.toml");
    config.dbus.talk = vec!["org.freedesktop.Notifications".into()];
    let unit = quadlet::generate_dbus_proxy_service("myenv", &config)
        .expect("proxy service should be generated");
    assert!(unit.contains("Restart=on-failure"));
    assert!(unit.contains("RestartSec=1s"));
}
#[test]
fn snapshot_quadlet_socket() {
    let config = load_config("full.toml");
    let q = quadlet::generate_socket(&config);
    insta::assert_snapshot!("quadlet_socket", q);
}
#[test]
fn snapshot_quadlet_build() {
    let config = load_config("full.toml");
    let cf_path = PathBuf::from("/home/user/.local/share/podbox/myenv/Containerfile");
    let q = quadlet::generate_build(&config, &cf_path);
    insta::assert_snapshot!("quadlet_build", q);
}
#[test]
fn snapshot_dbus_proxy_service() {
    let mut config = load_config("full.toml");
    config.dbus.talk = vec!["org.freedesktop.Notifications".into()];
    config.dbus.own = vec!["org.mpris.MediaPlayer2.podbox_app".into()];
    let unit = quadlet::generate_dbus_proxy_service("myenv", &config)
        .expect("proxy service should be generated");
    insta::assert_snapshot!("dbus_proxy_service", unit);
}
#[test]
fn compositor_service_structure() {
    let config = load_config("full.toml");
    let unit = quadlet::generate_compositor_service("myenv", &config)
        .expect("compositor service should be generated");
    assert!(unit.starts_with("[Unit]"));
    assert!(unit.contains("Description=Wayland Firewall Proxy for podbox container myenv"));
    assert!(unit.contains("PartOf=myenv.service"));
    assert!(unit.contains("[Service]"));
    assert!(unit.contains("Type=simple"));
    assert!(unit.contains("ExecStart="));
    assert!(unit.contains("compositor myenv"));
    assert!(unit.contains("Restart=on-failure"));
    assert!(unit.contains("RestartSec=1s"));
    assert!(!unit.contains("RuntimeDirectory=podbox"));
    assert!(unit.contains("[Install]"));
    assert!(unit.contains("WantedBy=myenv.service"));
}
#[test]
fn host_service_structure() {
    let unit = quadlet::generate_host_service("myenv");
    // NOT snapshot-tested because `generate_host_service` embeds
    // `current_exe()` which differs per test-binary path.
    assert!(unit.starts_with("[Unit]"));
    assert!(unit.contains("Description=podbox host socket server -- myenv"));
    assert!(unit.contains("[Service]"));
    assert!(unit.contains("Type=simple"));
    assert!(unit.contains("ExecStart="));
    assert!(unit.contains("serve myenv"));
    assert!(unit.contains("Restart=on-failure"));
    assert!(unit.contains("RestartSec=2s"));
    assert!(!unit.contains("RuntimeDirectory=podbox"));
    assert!(unit.contains("[Install]"));
    assert!(unit.contains("WantedBy=myenv.socket"));
}
