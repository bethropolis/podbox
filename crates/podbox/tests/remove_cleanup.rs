//! `podbox remove` must keep working on definitions that no longer
//! validate, and must not spray config diagnostics while scanning (`list`,
//! `stop --all`). Removal deletes state known by name; a legacy definition
//! written by an older podbox is not a blocker for it.

mod sandbox;

use sandbox::Sandbox;

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A definition written by podbox 0.8.x: `[dbus]` rules emitted while the
/// bus defaulted on, and no `integration.dbus` key at all (the old emitter
/// skipped it). 0.9's opt-in defaults read the absent key as off, which used
/// to strand the file behind a validation error it never chose.
fn legacy_husky_toml() -> String {
    r#"
schema_version = 1

[image]
base = "archlinux:latest"
name = "husky"

[container]
name = "husky"
home = "~/containers/husky"

[dbus]
talk = ["org.freedesktop.Notifications"]
"#
    .to_string()
}

#[test]
fn legacy_dbus_definition_is_removable() {
    let sb = Sandbox::new("rm-legacy-dbus");
    std::fs::write(sb.profiles().join("husky.toml"), legacy_husky_toml()).unwrap();
    std::fs::write(sb.root.join("bin/systemctl"), "#!/bin/sh\nexit 0\n").unwrap();
    make_executable(&sb.root.join("bin/systemctl"));

    // The strict path refuses (the D-Bus rule flips it back on only when the
    // key is absent, which this file is — so strict validation succeeds too;
    // this pins that `remove` never sees the error either way).
    let out = sb.run(&["remove", "husky", "--force"]);
    assert!(out.status.success(), "{}", sandbox::text(&out));
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("config validation failed"),
        "must not hit the D-Bus contradiction error: {}",
        sandbox::text(&out)
    );

    let calls = sb.podman_calls().join("\n");
    assert!(calls.contains("stop --ignore husky"), "{calls}");
    assert!(calls.contains("rm -f --ignore husky"), "{calls}");
}

#[test]
fn rm_continues_past_validation_errors() {
    let sb = Sandbox::new("rm-invalid");
    std::fs::write(
        sb.profiles().join("badbox.toml"),
        r#"
[image]
base = "archlinux:latest"
name = "badbox"

[container]
name = "badbox"
home = "~/containers/badbox"

[lifecycle]
idle_timeout = "bananas"
"#,
    )
    .unwrap();

    let out = sb.run(&["remove", "badbox", "--force"]);
    assert!(out.status.success(), "{}", sandbox::text(&out));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not applying strict validation"),
        "the demoted verdict must be visible: {}",
        sandbox::text(&out)
    );
}

#[test]
fn rm_refuses_a_structurally_broken_definition() {
    let sb = Sandbox::new("rm-broken");
    std::fs::write(
        sb.profiles().join("oops.toml"),
        r#"
[image]
base = "archlinux:latest"
name = "oops"

[container]
name = "oops"
home = "~/containers/oops"

[not-a-podbox-key]
x = 1
"#,
    )
    .unwrap();

    let out = sb.run(&["remove", "oops", "--force"]);
    assert!(!out.status.success(), "{}", sandbox::text(&out));
}

#[test]
fn list_stays_quiet_about_parse_warnings() {
    let sb = Sandbox::new("list-quiet");
    std::fs::write(
        sb.profiles().join("arch.toml"),
        legacy_husky_toml().replace("husky", "arch"),
    )
    .unwrap();
    std::fs::write(
        sb.profiles().join("dots.toml"),
        legacy_husky_toml().replace("husky", "dots"),
    )
    .unwrap();

    for cfg in ["arch", "dots"] {
        let on_stop = r#"
[lifecycle]
on_stop = "remove"
"#;
        let mut text = std::fs::read_to_string(sb.profiles().join(format!("{cfg}.toml"))).unwrap();
        text.push_str(on_stop);
        std::fs::write(sb.profiles().join(format!("{cfg}.toml")), text).unwrap();
    }

    let out = sb.run(&["list"]);
    assert!(out.status.success(), "{}", sandbox::text(&out));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("on_stop"),
        "list must not spray deprecation warnings: {stderr}"
    );
}

#[test]
fn stop_all_stays_quiet_about_parse_warnings() {
    let sb = Sandbox::new("stop-all-quiet");
    std::fs::write(
        sb.profiles().join("arch.toml"),
        legacy_husky_toml().replace("husky", "arch"),
    )
    .unwrap();
    let text = r#"
[lifecycle]
on_stop = "remove"
"#;
    let mut arch = std::fs::read_to_string(sb.profiles().join("arch.toml")).unwrap();
    arch.push_str(text);
    std::fs::write(sb.profiles().join("arch.toml"), arch).unwrap();

    let out = sb.run(&["stop", "--all"]);
    assert!(out.status.success(), "{}", sandbox::text(&out));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("on_stop"),
        "stop --all must not spray deprecation warnings: {stderr}"
    );
}

#[test]
fn rm_prints_a_deprecation_warning_once_at_most() {
    let sb = Sandbox::new("rm-once");
    std::fs::write(
        sb.profiles().join("arch.toml"),
        legacy_husky_toml().replace("husky", "arch"),
    )
    .unwrap();
    let on_stop = "\n[lifecycle]\non_stop = \"remove\"\n";
    let mut arch = std::fs::read_to_string(sb.profiles().join("arch.toml")).unwrap();
    arch.push_str(on_stop);
    std::fs::write(sb.profiles().join("arch.toml"), arch).unwrap();

    let out = sb.run(&["remove", "arch", "--force"]);
    assert!(out.status.success(), "{}", sandbox::text(&out));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let count = combined.matches("on_stop is deprecated").count();
    assert!(
        count <= 1,
        "deprecation warning printed {count} times: {combined}"
    );
}
