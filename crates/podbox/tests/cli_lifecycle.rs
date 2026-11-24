//! Lifecycle planning, entry guards, and watch stream contracts.

mod support;

use support::Sandbox;

#[test]
fn here_rejects_unmounted_current_directory_with_config_hint() {
    let sb = Sandbox::new(&["myenv"]);
    let out = sb
        .cmd()
        .args(["--dry-run", "-C", "myenv", "enter", "--here"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("is not accessible inside container 'myenv'"));
    assert!(stderr.contains("[container.mounts] extra"));
}

/// recover --dry-run prints the plan and touches nothing.
#[test]
fn recover_dry_run_prints_plan() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb
        .cmd()
        .args(["-C", "alpha", "recover", "--dry-run"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Recovery plan"));
    assert!(text.contains("daemon-reload"));
    assert!(text.contains("Quadlet"));
}

/// Contract P2: `restart --dry-run` prints the plan and touches nothing.
#[test]
fn restart_dry_run_prints_plan() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb
        .cmd()
        .args(["-C", "alpha", "restart", "--dry-run"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The fixture's inner container name is `myenv`, not the file stem.
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("restart myenv"),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// Contract P5: `watch --dry-run` prints the poll plan and exits.
#[test]
fn watch_dry_run_prints_plan() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb.cmd().args(["watch", "--dry-run"]).output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"event\":\"plan\"") && stdout.contains("\"interval_secs\":2"),
        "stdout: {stdout}"
    );
}

/// Contract P5: `watch --output ndjson` streams one JSON object per line.
/// Spawned and killed: the first lines are the initial snapshot.
#[test]
fn watch_ndjson_streams_snapshot_lines() {
    let sb = Sandbox::new(&["alpha"]);
    let mut child = sb
        .cmd()
        .args(["-C", "alpha", "watch", "--interval", "1"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(3));
    child.kill().ok();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.trim().is_empty(), "watch must print snapshot lines");
    for line in stdout.lines().take(3) {
        let v: serde_json::Value =
            serde_json::from_str(line).expect("every line must be one JSON object");
        assert_eq!(v["schema"], 1);
        assert_eq!(v["event"], "status");
        assert!(v["name"].is_string());
        assert!(v["status"].is_string());
    }
}

/// Consistency: watching an unknown name fails fast (exit 3) instead of
/// streaming `unbuilt` forever.
#[test]
fn watch_unknown_name_exits_three() {
    let sb = Sandbox::new(&[]);
    let out = sb
        .cmd()
        .args(["-C", "nosuch", "watch", "--output", "ndjson"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    assert_eq!(v["error"]["code"], "container_missing");
}
