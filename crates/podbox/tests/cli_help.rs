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

#[test]
fn runtime_commands_expose_here_and_env_without_transient_offline() {
    for command in ["enter", "exec"] {
        let out = podbox().args([command, "--help"]).output().unwrap();
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("--here"), "{command} should expose --here");
        assert!(stdout.contains("--env"), "{command} should expose --env");
        assert!(!stdout.contains("--offline"), "offline is declarative only");
    }
    let out = podbox().args(["run", "--help"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("--env"));
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

/// The global `--config <PATH>` override must reach every subcommand. Two
/// subcommands used to shadow it with a local `--config` boolean, which made
/// the "Hint: Use `--config <PATH>`" printed on a missing-config error
/// unusable for exactly the commands that showed the hint.
#[test]
fn global_config_path_is_not_shadowed_by_subcommands() {
    for subcommand in ["inspect", "remove"] {
        let out = podbox().args([subcommand, "--help"]).output().unwrap();
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        // Match declared options only: prose elsewhere may name the flag.
        let declares_config = stdout
            .lines()
            .any(|l| l.trim_start().starts_with("--config"));
        assert!(
            !declares_config,
            "{subcommand} must not declare its own --config flag"
        );
    }
}

/// `inspect` keeps a way to dump the resolved TOML, now spelled `--toml`.
#[test]
fn inspect_exposes_toml_and_quadlet_and_env() {
    let out = podbox().args(["inspect", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for flag in ["--toml", "--quadlet", "--env"] {
        assert!(stdout.contains(flag), "inspect should expose {flag}");
    }
}

/// `remove` still deletes the definition file, under an unambiguous name.
#[test]
fn remove_exposes_remove_config() {
    let out = podbox().args(["remove", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--remove-config"),
        "remove should expose --remove-config"
    );
}
