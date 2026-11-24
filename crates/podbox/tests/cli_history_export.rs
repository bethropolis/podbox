//! History log output and exported launcher/shim cleanup contracts.

mod support;

use support::Sandbox;

/// Seed a history log into the sandbox state dir and return the sandbox.
fn sb_with_history_log() -> Sandbox {
    let sb = Sandbox::new(&[]);
    let dir = sb.dir.join("state").join("podbox");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("history.log"),
        "2026-08-26T01:00:00Z\talpha\tbuild\t\n\
         2026-08-26T02:00:00Z\tbeta\tstart\t\n\
         2026-08-26T03:00:00Z\talpha\tstop\t\n",
    )
    .unwrap();
    sb
}

/// `history` prints newest-first entries from the state log.
#[test]
fn history_prints_entries_newest_first() {
    let sb = sb_with_history_log();
    let out = sb.cmd().arg("history").output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stop = stdout.find("stop").expect("stop entry present");
    let build = stdout.find("build").expect("build entry present");
    assert!(stop < build, "newest entry must print first");
}

/// NAME filter and `--limit` narrow the output.
#[test]
fn history_filters_by_name_and_limit() {
    let sb = sb_with_history_log();

    let out = sb.cmd().args(["history", "alpha"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("alpha"));
    assert!(!stdout.contains("beta"), "other containers filtered out");

    let out = sb.cmd().args(["history", "--limit", "1"]).output().unwrap();
    let rows = String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(2) // header + rule
        .count();
    assert_eq!(rows, 1, "limit caps printed entries");
}

/// JSON output is a machine-readable object on stdout only.
#[test]
fn history_json_output_parses() {
    let sb = sb_with_history_log();
    let out = sb
        .cmd()
        .args(["history", "--output", "json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid json");
    let events = v["history"].as_array().expect("history array");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["action"], "stop");
    assert_eq!(events[0]["name"], "alpha");
}

/// No log at all: empty success — reading history must never fail.
#[test]
fn history_without_log_is_empty_success() {
    let sb = Sandbox::new(&[]);
    let out = sb.cmd().arg("history").output().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

/// Contract P3: `export remove app` deletes one launcher; a second removal
/// fails cleanly. (Fixture inner name is `myenv`.)
#[test]
fn export_remove_app_roundtrip() {
    let sb = Sandbox::new(&["alpha"]);
    let apps_dir = sb.dir.join("data").join("applications");
    std::fs::create_dir_all(&apps_dir).unwrap();
    let planted = apps_dir.join("podbox-myenv-testapp.desktop");
    std::fs::write(
        &planted,
        "[Desktop Entry]\nName=Test (myenv)\nExec=podbox --container \"myenv\" run testapp\n",
    )
    .unwrap();

    let out = sb
        .cmd()
        .args(["-C", "alpha", "export", "remove", "app", "testapp"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!planted.exists(), "launcher must be gone");

    let out = sb
        .cmd()
        .args(["-C", "alpha", "export", "remove", "app", "testapp"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "second removal must fail");
}

/// Contract P3: `export remove bin` deletes one shim, and only one that
/// references the container. HOME is isolated so the real ~/.local/bin is
/// never touched.
#[test]
fn export_remove_bin_roundtrip() {
    let sb = Sandbox::new(&["alpha"]);
    let home = sb.dir.join("home");
    let bin_dir = home.join(".local/bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let shim = bin_dir.join("testtool");
    std::fs::write(
        &shim,
        "#!/bin/sh\nexec podbox --container \"myenv\" exec \"testtool\" \"$@\"\n",
    )
    .unwrap();
    // A same-named foreign file must survive: no marker, no removal.
    let foreign = bin_dir.join("other");
    std::fs::write(&foreign, "#!/bin/sh\necho hi\n").unwrap();

    let out = sb
        .cmd()
        .env("HOME", &home)
        .args(["-C", "alpha", "export", "remove", "bin", "testtool"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!shim.exists(), "shim must be gone");
    assert!(foreign.exists(), "foreign file must survive");

    let out = sb
        .cmd()
        .env("HOME", &home)
        .args(["-C", "alpha", "export", "remove", "bin", "other"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "foreign removal must fail");
}
