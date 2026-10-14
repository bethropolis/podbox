//! Container names reach file paths and podman arguments, so unsafe names
//! must be refused before any command runs.

use std::process::Command;

use assert_cmd::prelude::*;

/// Run `podbox` with an isolated XDG tree holding a sentinel file. Returns the
/// output and the sentinel path, so a test can check nothing was deleted.
fn run_isolated(args: &[&str]) -> (std::process::Output, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "podbox-name-test-{}-{}",
        std::process::id(),
        args.join("_").replace(['/', '.'], "")
    ));
    let _ = std::fs::remove_dir_all(&root);
    let quadlet = root.join("config/containers/systemd");
    std::fs::create_dir_all(&quadlet).unwrap();
    let sentinel = quadlet.join("keep-me.container");
    std::fs::write(&sentinel, "sentinel").unwrap();

    let output = Command::cargo_bin("podbox")
        .unwrap()
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_RUNTIME_DIR", root.join("run"))
        .args(args)
        .output()
        .unwrap();
    (output, sentinel)
}

#[test]
fn disable_with_parent_dir_name_is_refused_and_deletes_nothing() {
    let (out, sentinel) = run_isolated(&["disable", "--force", ".."]);
    assert_eq!(out.status.code(), Some(64), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid container name '..'"), "{stderr}");
    assert!(sentinel.exists(), "a file was removed despite the bad name");
}

#[test]
fn disable_with_current_dir_name_is_refused() {
    let (out, _) = run_isolated(&["disable", "--force", "."]);
    assert_eq!(out.status.code(), Some(64));
}

#[test]
fn leading_dash_name_is_refused_before_podman_sees_it() {
    let (out, _) = run_isolated(&["--container=-x", "status"]);
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid container name '-x'"));
}

#[test]
fn clone_checks_both_names() {
    let (out, _) = run_isolated(&["clone", "good-src", "a/b"]);
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid container name 'a/b'"));
}

#[test]
fn invalid_name_under_json_gives_error_code() {
    let (out, _) = run_isolated(&["status", "..", "--output", "json"]);
    assert_eq!(out.status.code(), Some(64));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"invalid_name\""), "{stdout}");
}
