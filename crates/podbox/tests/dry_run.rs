mod sandbox;

use std::process::Command;

use assert_cmd::prelude::*;

fn fixtures_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

#[test]
fn dry_build_shows_containerfile() {
    let mut cmd = Command::cargo_bin("podbox").unwrap();
    cmd.args([
        "--config",
        &fixtures_dir().join("full.toml").to_string_lossy(),
        "build",
        "--dry-run",
    ]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("FROM fedora:41"))
        .stdout(predicates::str::contains("podbox-guest"));
}

#[test]
fn dry_build_shows_podman_build_command() {
    let mut cmd = Command::cargo_bin("podbox").unwrap();
    cmd.args([
        "--config",
        &fixtures_dir().join("full.toml").to_string_lossy(),
        "build",
        "--dry-run",
    ]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("podman build"))
        .stdout(predicates::str::contains("localhost/podbox-myenv:latest"));
}

#[test]
fn dry_enable_shows_quadlet_container_section() {
    let mut cmd = Command::cargo_bin("podbox").unwrap();
    cmd.args([
        "--config",
        &fixtures_dir().join("full.toml").to_string_lossy(),
        "enable",
        "--dry-run",
    ]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("[Container]"))
        .stdout(predicates::str::contains("Environment=HOME=/home/%u"));
}

#[test]
fn dry_enable_shows_quadlet_socket_section() {
    let mut cmd = Command::cargo_bin("podbox").unwrap();
    cmd.args([
        "--config",
        &fixtures_dir().join("full.toml").to_string_lossy(),
        "enable",
        "--dry-run",
    ]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("[Socket]"))
        .stdout(predicates::str::contains(
            "ListenStream=%t/podbox/myenv.sock",
        ));
}

#[test]
fn dry_enable_shows_quadlet_build_section() {
    let mut cmd = Command::cargo_bin("podbox").unwrap();
    cmd.args([
        "--config",
        &fixtures_dir().join("full.toml").to_string_lossy(),
        "enable",
        "--dry-run",
    ]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("[Build]"))
        .stdout(predicates::str::contains(
            "ImageTag=localhost/podbox-myenv:latest",
        ));
}

#[test]
fn dry_shell_shows_podman_exec() {
    let mut cmd = Command::cargo_bin("podbox").unwrap();
    cmd.args([
        "--config",
        &fixtures_dir().join("full.toml").to_string_lossy(),
        "shell",
        "--dry-run",
    ]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("podman exec"));
}

#[test]
fn dry_stop_shows_systemctl_stop() {
    let mut cmd = Command::cargo_bin("podbox").unwrap();
    cmd.args([
        "--config",
        &fixtures_dir().join("full.toml").to_string_lossy(),
        "stop",
        "--dry-run",
    ]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("systemctl --user stop"));
}

// Mutating commands under `--dry-run`. Each runs against an isolated XDG tree
// and a stub podman that logs its calls. Assertions: the user's files are
// unchanged and no state-changing podman verb ran.

use sandbox::{Sandbox, text};

#[test]
fn dry_config_set_leaves_the_file_unchanged() {
    let sb = Sandbox::new("dry-config-set");
    let path = sb.write_config("dev", "dev");
    let before = std::fs::read_to_string(&path).unwrap();

    let out = sb.run(&[
        "-C",
        "dev",
        "--dry-run",
        "config",
        "set",
        "container.memory",
        "4g",
    ]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("would: set"), "{}", text(&out));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[test]
fn dry_disable_removes_no_quadlet_files() {
    let sb = Sandbox::new("dry-disable");
    sb.write_config("dev", "dev");
    let quadlet = sb.root.join("config/containers/systemd");
    std::fs::create_dir_all(&quadlet).unwrap();
    let sentinel = quadlet.join("dev.container");
    std::fs::write(&sentinel, "unit").unwrap();

    let out = sb.run(&["-C", "dev", "--dry-run", "disable", "--force"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("would: remove"), "{}", text(&out));
    assert!(sentinel.exists(), "dry-run disable removed a unit file");
    assert!(sb.mutating_podman_calls().is_empty());
}

#[test]
fn dry_restore_makes_no_state_changing_podman_calls() {
    let sb = Sandbox::new("dry-restore");
    sb.write_config("dev", "dev");

    let out = sb.run(&["-C", "dev", "--dry-run", "restore", "t1"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("would: stop"), "{}", text(&out));
    assert!(
        sb.mutating_podman_calls().is_empty(),
        "{:?}",
        sb.podman_calls()
    );
}

#[test]
fn dry_snapshot_create_commits_nothing_and_writes_no_metadata() {
    let sb = Sandbox::new("dry-snapshot");
    sb.write_config("dev", "dev");

    let out = sb.run(&[
        "-C",
        "dev",
        "--dry-run",
        "snapshot",
        "create",
        "--tag",
        "t1",
    ]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("would: commit"), "{}", text(&out));
    assert!(
        sb.mutating_podman_calls().is_empty(),
        "{:?}",
        sb.podman_calls()
    );
    assert!(!sb.root.join("data/podbox/snapshots/dev").exists());
}

#[test]
fn dry_profile_import_force_refuses_and_keeps_the_profile() {
    let sb = Sandbox::new("dry-profile");
    let existing = sb.profiles().join("cachy.toml");
    std::fs::write(&existing, "keep = true\n").unwrap();
    let src = sb.root.join("incoming.toml");
    std::fs::write(&src, "x = 1\n").unwrap();

    let out = sb.run(&[
        "--dry-run",
        "profile",
        "import",
        src.to_str().unwrap(),
        "--name",
        "cachy",
        "--force",
    ]);
    assert!(!out.status.success(), "{}", text(&out));
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "keep = true\n");
}

#[test]
fn dry_export_refuses_rather_than_writing_desktop_files() {
    let sb = Sandbox::new("dry-export");
    sb.write_config("dev", "dev");

    let out = sb.run(&["-C", "dev", "--dry-run", "export", "app", "foo"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains("--dry-run is not supported"),
        "{}",
        text(&out)
    );
    assert!(sb.mutating_podman_calls().is_empty());
}

#[test]
fn dry_doctor_fix_reports_without_fixing() {
    let sb = Sandbox::new("dry-doctor");
    sb.write_config("dev", "dev");

    let out = sb.run(&["-C", "dev", "--dry-run", "doctor", "--fix"]);
    assert!(
        text(&out).contains("--fix is skipped under --dry-run"),
        "{}",
        text(&out)
    );
    assert!(sb.mutating_podman_calls().is_empty());
}

/// Non-interactive `cache prune` (no name) must refuse without `--yes` and
/// say so, and `cache prune --yes` must actually prune: volumes are named
/// caches the operator shared on purpose, but a scripted wipe still needs an
/// explicit flag because there is nobody to ask.
#[test]
fn cache_prune_needs_confirmation_unless_yes() {
    let sb = Sandbox::new("cache-prune");
    sb.write_config("dev", "dev");
    // The default stub prints nothing; `cache prune` needs a `volume ls`
    // that answers, so swap in a stub that lists two cache volumes.
    let log = sb.root.join("podman.log");
    let podman = sb.root.join("bin").join("podman");
    std::fs::write(
        &podman,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\ncase \"$*\" in\n  *\"volume ls\"*) printf 'podbox-cache-cargo\\npodbox-cache-npm\\n';;\nesac\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&podman, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let out = sb.run(&["cache", "prune"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains("requires confirmation in a terminal") && text(&out).contains("--yes"),
        "refusal should name the confirmation rule and the flag: {}",
        text(&out)
    );
    assert!(
        sb.podman_calls()
            .iter()
            .all(|c| !c.starts_with("volume rm ")),
        "nothing may be removed without --yes: {:?}",
        sb.podman_calls()
    );

    let out = sb.run(&["cache", "prune", "--yes"]);
    assert!(out.status.success(), "{}", text(&out));
    let removed: Vec<String> = sb
        .podman_calls()
        .into_iter()
        .filter(|c| c.starts_with("volume rm "))
        .collect();
    assert!(
        removed.contains(&"volume rm podbox-cache-cargo".to_string()),
        "expected the default cache volumes to be pruned, got {removed:?}"
    );
}

#[test]
fn dry_build_edit_does_not_open_the_editor() {
    let sb = Sandbox::new("dry-edit");
    sb.write_config("dev", "dev");
    let marker = sb.root.join("editor-ran");
    let editor = sb.root.join("fake-editor.sh");
    std::fs::write(
        &editor,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let out = sb.run_env(
        &["-C", "dev", "--dry-run", "build", "--edit"],
        &[("EDITOR", editor.to_str().unwrap())],
    );
    assert!(text(&out).contains("would: open"), "{}", text(&out));
    assert!(!marker.exists(), "the editor ran under --dry-run");
}

#[test]
fn dry_diff_apply_leaves_the_definition_unchanged() {
    let sb = Sandbox::new("dry-diff");
    // A declared package the stub container does not have, so `diff` reports
    // drift and `--apply` would rewrite the install list.
    let mut cfg = podbox::config::Config::embedded();
    cfg.container.name = "dev".into();
    cfg.image.name = "dev".into();
    cfg.image.packages.install = vec!["ripgrep".into()];
    let path = sb.profiles().join("dev.toml");
    std::fs::write(&path, toml::to_string_pretty(&cfg).unwrap()).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();

    let out = sb.run(&["-C", "dev", "--dry-run", "diff", "--apply"]);
    assert!(text(&out).contains("would: rewrite"), "{}", text(&out));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[test]
fn dry_run_under_json_keeps_stdout_a_single_document() {
    let sb = Sandbox::new("dry-json");
    let mut cfg = podbox::config::Config::embedded();
    cfg.container.name = "dev".into();
    cfg.image.name = "dev".into();
    cfg.image.packages.install = vec!["ripgrep".into()];
    std::fs::write(
        sb.profiles().join("dev.toml"),
        toml::to_string_pretty(&cfg).unwrap(),
    )
    .unwrap();

    let out = sb.run(&[
        "-C",
        "dev",
        "--dry-run",
        "diff",
        "--apply",
        "--output",
        "json",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(stdout.trim());
    assert!(parsed.is_ok(), "stdout is not one JSON document: {stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("would: rewrite"), "{stderr}");
    assert!(
        !stdout.contains("would:"),
        "plan leaked to stdout: {stdout}"
    );
}
