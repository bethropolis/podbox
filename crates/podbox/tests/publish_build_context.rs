//! `podbox publish` build-context naming.
//!
//! Offline: the invalid-name path must fail before any registry probe, so
//! these need no buildx, podman, or network.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use assert_cmd::prelude::*;

fn write_project(
    dir: &Path,
    manifest_containerfile: Option<&str>,
    entry_containerfile: Option<&str>,
) {
    let mut manifest = String::from("schema_version = 1\nregistry = \"ghcr.io\"\n");
    if let Some(name) = manifest_containerfile {
        writeln!(manifest, "containerfile = \"{name}\"").unwrap();
    }
    manifest.push_str("\n[images.dev]\nconfig = \"dev.toml\"\n");
    if let Some(name) = entry_containerfile {
        writeln!(manifest, "containerfile = \"{name}\"").unwrap();
    }
    std::fs::write(dir.join("images.toml"), manifest).unwrap();

    std::fs::write(
        dir.join("dev.toml"),
        r#"[image]
base = "alpine:latest"
name = "dev"

[container]
name = "dev"
home = "~/dev"
"#,
    )
    .unwrap();
}

fn publish(dir: &Path, extra: &[&str]) -> std::process::Output {
    Command::cargo_bin("podbox")
        .unwrap()
        .args([
            "publish",
            "--dir",
            dir.to_str().unwrap(),
            "--list",
            "--owner",
            "o",
            "--repo",
            "r",
            "--sha",
            "abc1234",
            "--ref-name",
            "main",
            "--ref-type",
            "branch",
        ])
        .args(extra)
        .output()
        .unwrap()
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn invalid_cli_name_fails_before_any_probe() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path(), None, None);

    let out = publish(dir.path(), &["--containerfile", "../escape"]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("invalid Containerfile name"),
        "expected the name in the error, got: {}",
        stderr(&out)
    );
}

#[test]
fn invalid_per_image_name_fails() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path(), None, Some("a/b"));

    let out = publish(dir.path(), &[]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("invalid Containerfile name"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn invalid_manifest_wide_name_fails() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path(), Some("-x"), None);

    let out = publish(dir.path(), &[]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("invalid Containerfile name"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn valid_custom_name_is_accepted_under_list() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path(), None, Some("Dockerfile.custom"));

    let out = publish(dir.path(), &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}
