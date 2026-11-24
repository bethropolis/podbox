//! Config read/write/validation and environment creation contracts.

mod support;

use std::path::PathBuf;

use support::Sandbox;

/// Contract P4: `config get` reads effective values, raw in text mode.
#[test]
fn config_get_roundtrip() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb
        .cmd()
        .args(["-C", "alpha", "config", "get", "container.name"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "myenv");

    let out = sb
        .cmd()
        .args([
            "-C",
            "alpha",
            "config",
            "get",
            "container.shell",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    assert_eq!(v["schema"], 1);
    assert_eq!(v["data"], "fish");

    let out = sb
        .cmd()
        .args(["-C", "alpha", "config", "get", "container.nope"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "unknown key must fail");
}

/// Contract P4: `config set` writes through toml_edit (comments survive),
/// validates values, and rejects unknown keys without touching the file.
#[test]
fn config_set_roundtrip() {
    let sb = Sandbox::new(&["alpha"]);
    let path = sb.dir.join("podbox").join("alpha.toml");
    std::fs::write(
        &path,
        format!(
            "{}\n# precious comment\n",
            std::fs::read_to_string(&path).unwrap()
        ),
    )
    .unwrap();

    let out = sb
        .cmd()
        .args(["-C", "alpha", "config", "set", "container.memory", "4G"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(
        content.contains("# precious comment"),
        "comment must survive"
    );
    assert!(content.contains("memory"), "new key must land");

    let out = sb
        .cmd()
        .args(["-C", "alpha", "config", "get", "container.memory"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4G");

    // TOML-typed values stay typed: boolean in, boolean out.
    let out = sb
        .cmd()
        .args(["-C", "alpha", "config", "set", "integration.gpu", "true"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let out = sb
        .cmd()
        .args([
            "-C",
            "alpha",
            "config",
            "get",
            "integration.gpu",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    assert_eq!(v["data"], true);

    // Wrong-typed values fail validation instead of writing.
    let out = sb
        .cmd()
        .args(["-C", "alpha", "config", "set", "container.cpus", "8"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "integer for a string field must fail"
    );

    // Unknown key: clean failure, file untouched.
    let before = std::fs::read_to_string(&path).unwrap();
    let out = sb
        .cmd()
        .args(["-C", "alpha", "config", "set", "container.memroy", "4G"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "typo key must fail");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);

    // Image keys report the rebuild.
    let out = sb
        .cmd()
        .args(["-C", "alpha", "config", "set", "image.base", "fedora:42"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("build"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Contract P4: `config validate` reports errors with line numbers.
#[test]
fn config_validate_reports_lines() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb
        .cmd()
        .args(["-C", "alpha", "config", "validate"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("valid"),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    // Semantic break with a known line.
    let bad = sb.dir.join("podbox").join("bad.toml");
    let mut text = std::fs::read_to_string(sb.dir.join("podbox").join("alpha.toml")).unwrap();
    text = text.replace("[container]\n", "[container]\nmemory = \"bogus\"\n");
    let expected_line = text.lines().position(|l| l.contains("memory = ")).unwrap() + 1;
    std::fs::write(&bad, &text).unwrap();
    let out = sb
        .cmd()
        .args(["-C", "bad", "config", "validate", "--output", "json"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "invalid config must fail");
    let v: serde_json::Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim())
        .expect("stdout must stay a single JSON document");
    assert_eq!(v["schema"], 1);
    assert_eq!(v["valid"], false);
    let first = &v["errors"][0];
    assert_eq!(first["field"], "container.memory");
    assert_eq!(first["line"], expected_line);

    // Syntax break: unlocated field, located position.
    std::fs::write(
        sb.dir.join("podbox").join("broken.toml"),
        "[container\nname = 1\n",
    )
    .unwrap();
    let out = sb
        .cmd()
        .args(["-C", "broken", "config", "validate", "--output", "json"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    assert_eq!(v["valid"], false);
    assert!(v["errors"][0]["field"].is_null());
    assert!(v["errors"][0]["line"].is_number());
}

/// `config validate` takes an optional NAME: the named definition, else the
/// active context, else the single/auto-detected config. A name with no
/// definition exits 3 (`container_missing`), never falls back to another box.
#[test]
fn config_validate_accepts_a_positional_name() {
    let sb = Sandbox::new(&["alpha", "beta"]);
    // Positional name picks that definition.
    let out = sb
        .cmd()
        .args(["config", "validate", "alpha"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("valid"));

    // A bad named definition fails on its own merits.
    let mut text = std::fs::read_to_string(sb.dir.join("podbox").join("beta.toml")).unwrap();
    text = text.replace("[container]\n", "[container]\nmemory = \"bogus\"\n");
    std::fs::write(sb.dir.join("podbox").join("beta.toml"), &text).unwrap();
    let out = sb
        .cmd()
        .args(["config", "validate", "beta", "--output", "json"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "invalid config must fail");
    let v: serde_json::Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim())
        .expect("stdout must stay a single JSON document");
    assert_eq!(v["schema"], 1);
    assert_eq!(v["valid"], false);
    assert_eq!(v["errors"][0]["field"], "container.memory");

    // No definition under that name: exit 3, and no silent fallback.
    let out = sb
        .cmd()
        .args(["config", "validate", "nosuch"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3), "missing config exits 3");

    // The active context is the fallback when no name is given.
    let out = sb.cmd().args(["use", "beta"]).output().unwrap();
    assert!(out.status.success());
    let out = sb.cmd().args(["config", "validate"]).output().unwrap();
    assert!(
        !out.status.success(),
        "the active (invalid) context is what gets validated"
    );
}

/// Contract P4: `create --yes --dry-run` never prompts and derives names.
#[test]
fn create_yes_dry_run() {
    let sb = Sandbox::new(&[]);
    let out = sb
        .cmd()
        .args([
            "create",
            "fedora",
            "--name",
            "testbox",
            "--yes",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("Would create config"),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    let out = sb
        .cmd()
        .args([
            "create",
            "docker.io/library/fedora:41",
            "--yes",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("fedora-41"),
        "derived name missing: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    let out = sb
        .cmd()
        .args(["create", "docker.io/library/fedora:41", "--dry-run"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("podbox init"),
        "suggestion missing: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// `create <name>` with an on-disk profiles/<name>.toml reuses it instead of
/// rebuilding from the bundled template (the Studio quick-start flow).
#[test]
fn create_reuses_on_disk_config_over_bundled_profile() {
    let sb = Sandbox::new(&[]);
    let profiles = sb.dir.join("podbox").join("profiles");
    std::fs::create_dir_all(&profiles).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/full.toml");
    std::fs::copy(&fixture, profiles.join("myenv.toml")).unwrap();

    let out = sb
        .cmd()
        .args(["--dry-run", "create", "myenv"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Reusing existing config"),
        "stderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Would reuse existing config"),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("fedora:41"),
        "must build from the user's file, got:\n{stdout}"
    );
}

/// `create --import` validates and stages a file without writing on dry-run.
#[test]
fn create_import_dry_run_stages_nothing() {
    let sb = Sandbox::new(&[]);
    let src = sb.dir.join("studio.toml");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/full.toml");
    std::fs::copy(&fixture, &src).unwrap();

    let out = sb
        .cmd()
        .args(["--dry-run", "create", "--import", src.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Would import"), "stdout:\n{stdout}");
    assert!(
        !sb.dir.join("podbox").join("profiles").exists(),
        "dry-run must not create profiles/"
    );
}

/// `create --import` rejects garbage before touching profiles/.
#[test]
fn create_import_rejects_invalid_toml() {
    let sb = Sandbox::new(&[]);
    let src = sb.dir.join("bad.toml");
    std::fs::write(&src, "[image]\nbase = 42\n").unwrap();

    let out = sb
        .cmd()
        .args(["create", "--import", src.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        !sb.dir.join("podbox").join("profiles").exists(),
        "invalid import must not create profiles/"
    );
}
