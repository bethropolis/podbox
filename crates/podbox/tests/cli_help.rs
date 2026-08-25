//! PR 1 (CLI experience): help ordering/grouping, aliases, hidden internals.
//!
//! These tests exercise only clap-level behavior (`--help`, subcommand
//! resolution), so they do not require podman to be installed.

use std::process::Command;

use assert_cmd::prelude::*;

fn podbox() -> Command {
    Command::cargo_bin("podbox").unwrap()
}

/// Daily-path commands must appear before lifecycle/management commands, and
/// systemd internals must stay out of the default help listing.
#[test]
fn help_orders_daily_path_first_and_hides_internals() {
    let out = podbox().args(["--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Internals are hidden from the default command list.
    let commands_section = stdout
        .split("Commands:")
        .nth(1)
        .and_then(|s| s.split("\n\n").next())
        .unwrap_or_default();
    assert!(
        !commands_section.contains("serve"),
        "serve should be hidden from default help"
    );
    assert!(
        !commands_section.contains("compositor"),
        "compositor should be hidden from default help"
    );

    // Relative group ordering: get started -> day to day -> change -> remove.
    // Anchored on "\n  <name> " so words inside about-text can't false-match.
    let pos = |name: &str| stdout.find(&format!("\n  {name} ")).expect(name);
    assert!(pos("create") < pos("enter"));
    assert!(pos("enter") < pos("exec"));
    assert!(pos("exec") < pos("start"));
    assert!(pos("start") < pos("list"));
    assert!(pos("list") < pos("build"));
    assert!(pos("build") < pos("logs"));
    assert!(pos("clone") < pos("remove"));
    assert!(pos("remove") < pos("use"));
}

#[test]
fn help_advertises_visible_aliases() {
    let out = podbox().args(["--help"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[aliases: ls]"), "list alias");
    assert!(stdout.contains("[aliases: rm]"), "remove alias");
    assert!(stdout.contains("[aliases: shell]"), "enter alias");
}

#[test]
fn help_shows_workflow_hints() {
    let out = podbox().args(["--help"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Common workflow:"));
    assert!(stdout.contains("podbox enter"));
}

/// `enter` is canonical; `shell` resolves to the same command.
#[test]
fn shell_alias_resolves_to_enter() {
    let out = podbox().args(["shell", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Open an interactive shell in the container"),
        "shell alias should resolve to enter"
    );
}

/// `rm` resolves to `remove`.
#[test]
fn rm_alias_resolves_to_remove() {
    let out = podbox().args(["rm", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Remove the container"));
}
