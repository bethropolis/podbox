//! Which config a command acts on. A wrong pick here can delete the wrong
//! box, so these tests pin the resolution rules with an isolated XDG tree.

mod sandbox;

use sandbox::{Sandbox, text};

#[test]
fn serve_targets_its_named_box_when_several_configs_exist() {
    let sb = Sandbox::new("serve-multi");
    sb.write_config("dev", "dev");
    sb.write_config("work", "work");

    // Without the fix, generic resolution bails with "Multiple container configs".
    let out = sb.run(&["--dry-run", "serve", "dev"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("podbox serve dev"));
}

#[test]
fn serve_ignores_a_broken_config_belonging_to_another_box() {
    let sb = Sandbox::new("serve-broken");
    sb.write_config("dev", "dev");
    std::fs::write(sb.profiles().join("work.toml"), "this is = not [valid").unwrap();

    let out = sb.run(&["--dry-run", "serve", "dev"]);
    assert!(out.status.success(), "{}", text(&out));
}

#[test]
fn stats_targets_the_named_box() {
    let sb = Sandbox::new("stats-name");
    sb.write_config("dev", "dev");
    sb.write_config("work", "work");

    // `foo` has no config. Before the fix the name was ignored and `stats`
    // silently picked a box, so this must be a missing-container error.
    let out = sb.run(&["stats", "foo", "--no-stream"]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out));
    assert!(text(&out).contains("foo"), "{}", text(&out));
}

#[test]
fn explicit_missing_config_is_a_hard_error_and_removes_nothing() {
    let sb = Sandbox::new("missing-config");
    sb.write_config("dev", "dev");
    let typo = sb.root.join("typo.toml");

    let out = sb.run(&[
        "--config",
        typo.to_str().unwrap(),
        "status",
        "--output",
        "json",
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    let body = text(&out);
    assert!(body.contains("config_missing"), "{body}");
    assert!(sb.profiles().join("dev.toml").exists());
}

#[test]
fn explicit_missing_config_never_falls_back_to_the_default_box() {
    let sb = Sandbox::new("missing-rm");
    sb.write_config("dev", "dev");
    let typo = sb.root.join("typo.toml");

    let out = sb.run(&["--config", typo.to_str().unwrap(), "rm", "--force"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(sb.profiles().join("dev.toml").exists());
}
