//! Machine-readable JSON/NDJSON output and structured error contracts.

mod support;

use support::Sandbox;

/// Contract P1: every podbox-owned `--output json` payload carries
/// `"schema": 1`, and stdout stays a single JSON document.
#[test]
fn json_readers_carry_schema() {
    let sb = Sandbox::new(&["alpha"]);
    for args in [
        vec!["list", "--output", "json"],
        vec!["history", "--output", "json"],
        vec!["-C", "alpha", "snapshot", "list", "--output", "json"],
    ] {
        let out = sb.cmd().args(&args).output().unwrap();
        assert!(
            out.status.success(),
            "{args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: serde_json::Value =
            serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim())
                .expect("{args:?} must print a single JSON document");
        assert_eq!(v["schema"], 1, "{args:?} missing schema stamp");
    }
}

/// Contract P1: JSON-mode failure prints the structured error envelope on
/// stdout with the documented exit code; diagnostics stay on stderr.
#[test]
fn json_error_envelope_for_missing_container() {
    let sb = Sandbox::new(&[]);
    // `status` resolves the named config; a missing one is exit 3.
    let out = sb
        .cmd()
        .args(["-C", "nosuch", "status", "--output", "json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let v: serde_json::Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim())
        .expect("stdout must be a single JSON error document");
    assert_eq!(v["schema"], 1);
    assert_eq!(v["error"]["code"], "container_missing");
    assert!(v["error"]["message"].is_string());
    assert!(
        !String::from_utf8_lossy(&out.stderr).is_empty(),
        "human diagnostics must stay on stderr"
    );
}

/// Contract P2: `export list --output json` reports declared and exported
/// items with the schema stamp.
#[test]
fn export_list_json_shape() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb
        .cmd()
        .args(["-C", "alpha", "export", "list", "--output", "json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim())
        .expect("export list must print a single JSON document");
    assert_eq!(v["schema"], 1);
    let apps = v["apps"].as_array().expect("apps array");
    assert!(
        apps.iter()
            .any(|a| a["id"] == "gedit" && a["exported"] == false),
        "declared app missing: {apps:?}"
    );
    let bins = v["bins"].as_array().expect("bins array");
    assert!(
        bins.iter()
            .any(|b| b["name"] == "rg" && b["exported"] == false),
        "declared bin missing: {bins:?}"
    );
}

/// Contract P2: `status --output json` carries the base image reference.
#[test]
fn status_json_has_base_image() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb
        .cmd()
        .args(["-C", "alpha", "status", "--output", "json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim())
        .expect("status must print a single JSON document");
    assert_eq!(v["schema"], 1);
    assert!(v["base_image"].is_string(), "base_image missing: {v}");
}

/// Contract P3: `info --output json` reports versions and capabilities,
/// needing no config and no container.
#[test]
fn info_json_shape() {
    let sb = Sandbox::new(&[]);
    let out = sb
        .cmd()
        .args(["info", "--output", "json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim())
        .expect("info must print a single JSON document");
    assert_eq!(v["schema"], 1);
    assert!(v["podbox"].is_string(), "podbox version missing: {v}");
    let version = v["podbox"].as_str().unwrap();
    assert!(
        !version.starts_with('v'),
        "podbox must report a bare semver, got {version:?}"
    );
    assert!(v["build"].is_string(), "build/describe string missing: {v}");
    let features = v["features"].as_array().expect("features array");
    assert!(
        features.iter().any(|f| f == "restart"),
        "features missing restart: {features:?}"
    );
    assert!(
        features.iter().any(|f| f == "info_build"),
        "features missing info_build: {features:?}"
    );
}

/// Contract P5: unsupported output values are rejected by clap (exit 2),
/// not by the handler after it starts.
#[test]
fn ndjson_rejected_outside_streams() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb
        .cmd()
        .args(["list", "--output", "ndjson"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("invalid value 'ndjson'"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Contract P5: `build` advertises only text and ndjson; json is a clap
/// error, and dry-run ndjson still prints the human plan.
#[test]
fn build_output_modes() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb
        .cmd()
        .args(["-C", "alpha", "build", "--output", "json", "--dry-run"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("invalid value 'json'"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = sb
        .cmd()
        .args(["-C", "alpha", "build", "--dry-run", "--output", "ndjson"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("Build context"),
        "dry-run prints the human plan: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// Consistency: `list` shows runtime names (`container.name`), not file
/// stems — matching `status`, units, and exports. The fixture file is
/// `alpha.toml` but its inner name is `myenv`.
#[test]
fn list_shows_runtime_names() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb
        .cmd()
        .args(["list", "--output", "json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    let names: Vec<&str> = v["containers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["myenv"]);
}
