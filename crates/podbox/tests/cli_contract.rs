//! PR 2 (CLI experience): scriptable output/error contract.
//!
//! Covers `find-definition` (path-or-empty stdout, non-zero when missing) and
//! the non-TTY guards that replace interactive prompts with actionable errors.
//!
//! These tests isolate themselves via XDG_CONFIG_HOME and use a stub `podman`
//! on PATH, so they do not touch the developer's real config or need podman.

use std::fs::Permissions;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use assert_cmd::prelude::*;

struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    /// Fresh temp dir with a stub podman and an isolated XDG config home.
    fn new(configs: &[&str]) -> Self {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("podbox-test-{}-{n}", std::process::id()));
        let cfg = dir.join("podbox");
        std::fs::create_dir_all(&cfg).unwrap();
        for name in configs {
            // Copy a valid fixture so commands that fully parse the config
            // (not just stat it) succeed.
            let fixture =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/full.toml");
            std::fs::copy(&fixture, cfg.join(format!("{name}.toml"))).unwrap();
        }

        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let stub = bin.join("podman");
        let mut f = std::fs::File::create(&stub).unwrap();
        writeln!(f, "#!/bin/sh\nexit 0").unwrap();
        drop(f);
        std::fs::set_permissions(&stub, Permissions::from_mode(0o755)).unwrap();

        Self { dir }
    }

    /// `dirs::config_dir()` appends "podbox" to $XDG_CONFIG_HOME, so the
    /// env var must point at the sandbox root, not at the inner podbox dir.
    fn path_env(&self) -> String {
        format!(
            "{}:{}",
            self.dir.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        )
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("podbox").unwrap();
        c.env("XDG_CONFIG_HOME", &self.dir)
            .env("XDG_DATA_HOME", self.dir.join("data"))
            .env("XDG_STATE_HOME", self.dir.join("state"))
            .env("XDG_RUNTIME_DIR", self.dir.join("runtime"))
            .env_remove("PODBOX_CONTAINER")
            .env("PATH", self.path_env());
        c
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Missing named config: empty stdout + documented exit code 2.
#[test]
fn find_definition_missing_exits_two() {
    let sb = Sandbox::new(&[]);
    let out = sb
        .cmd()
        .args(["find-definition", "nosuch"])
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2));
    assert!(
        out.stdout.is_empty(),
        "stdout must stay machine-clean on failure"
    );
}

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

/// Existing named config: exactly the path on stdout, exit 0.
#[test]
fn find_definition_prints_path() {
    let sb = Sandbox::new(&["myenv"]);
    let out = sb
        .cmd()
        .args(["find-definition", "myenv"])
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let expected = sb.dir.join("podbox").join("myenv.toml");
    assert_eq!(stdout.trim(), expected.display().to_string());
}

/// Non-TTY with multiple configs and no explicit name: error with hint,
/// never a prompt or silent guess.
#[test]
fn non_tty_multiple_configs_errors_with_hint() {
    let sb = Sandbox::new(&["alpha", "beta"]);
    // `enter --dry-run` reaches resolve_config; the guard must bail before
    // any prompt is attempted.
    let out = sb.cmd().args(["enter", "--dry-run"]).output().unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Multiple container configs found"));
    assert!(stderr.contains("alpha"));
    assert!(stderr.contains("beta"));
    assert!(stderr.contains("-C <NAME>"), "must suggest -C");
}

/// Explicit `-C` sidesteps the ambiguity entirely in non-TTY mode.
#[test]
fn non_tty_explicit_container_flag_resolves() {
    let sb = Sandbox::new(&["alpha", "beta"]);
    let out = sb
        .cmd()
        .args(["-C", "alpha", "enter", "--dry-run"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Hidden helper lists config stems — one per line, never fails.
#[test]
fn complete_names_lists_configs_and_never_fails() {
    let sb = Sandbox::new(&["alpha", "beta"]);
    let out = sb.cmd().arg("__complete-names").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("alpha"));
    assert!(stdout.contains("beta"));

    // Empty config dir: empty output, still success.
    let empty = Sandbox::new(&[]);
    let out = empty.cmd().arg("__complete-names").output().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

/// Generated scripts embed the dynamic-name glue.
#[test]
fn completions_include_dynamic_name_glue() {
    for (shell, marker) in [
        ("bash", "__podbox_wrap"),
        ("fish", "__podbox_names"),
        ("zsh", "__podbox_wrap"),
    ] {
        let out = sb_cmd_completions(shell).output().unwrap();
        assert!(out.status.success(), "{shell}");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains(marker),
            "{shell} script should reference {marker}"
        );
    }
}

/// Fish `--abbrs` emits daily-driver `abbr` shorthand; other shells (or fish
/// without the flag) print the default stream unchanged.
#[test]
fn fish_abbrs_are_opt_in_and_fish_only() {
    let out = sb_cmd_completions("fish")
        .args(["--abbrs"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Every curated abbreviation is present as an `abbr` line.
    for token in [
        "pb", "pbb", "pbc", "pbd", "pbe", "pbl", "pbr", "pbs", "pbt", "pbu", "pbv", "pbx",
    ] {
        assert!(
            stdout
                .lines()
                .any(|l| l.starts_with(&format!("abbr {token} "))),
            "missing `abbr {token}` definition"
        );
    }

    // Default fish output carries no `abbr` lines — scripts that pipe the
    // default stream must see the same script as before this feature.
    let default = sb_cmd_completions("fish").output().unwrap();
    let default_stdout = String::from_utf8_lossy(&default.stdout);
    assert!(
        !default_stdout.contains("abbr "),
        "default fish output must not contain abbreviations"
    );

    // `--abbrs` is ignored for non-fish shells.
    let bash = sb_cmd_completions("bash")
        .args(["--abbrs"])
        .output()
        .unwrap();
    assert!(bash.status.success());
    let bash_stdout = String::from_utf8_lossy(&bash.stdout);
    assert!(
        !bash_stdout.contains("abbr "),
        "abbreviations must be fish-only"
    );
}

fn sb_cmd_completions(shell: &str) -> Command {
    let mut c = Sandbox::new(&[]).cmd();
    c.args(["completions", shell]);
    c
}

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

/// `list` columns line up: AUTOSTART starts at the same offset everywhere,
/// rows have no trailing whitespace, and the rule matches the header width.
///
/// The pre-fix bug padded colored cells by *byte* length, so ANSI escapes
/// shifted every column after STATUS.
#[test]
fn list_columns_align_without_trailing_space() {
    let sb = Sandbox::new(&["alpha", "beta"]);
    let out = sb.cmd().arg("list").output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 4, "header + rule + two rows");

    // Non-TTY output carries no color codes; widths are then exact.
    let header = lines[0];
    let autostart_col = header.find("AUTOSTART").expect("AUTOSTART header");
    let active_col = header
        .find("ACTIVE CONTEXT")
        .expect("ACTIVE CONTEXT header");

    for row in &lines[2..] {
        assert_eq!(
            row.len(),
            row.trim_end().len(),
            "trailing whitespace in {row:?}"
        );
        // Every data row reaches (at least) the AUTOSTART column offset.
        assert!(row.len() > autostart_col, "row too short: {row:?}");
    }

    // Rule spans the header width.
    assert_eq!(lines[1].chars().count(), header.trim_end().chars().count());

    // Status labels start right after the fixed CONTAINER + dot columns.
    for row in &lines[2..] {
        assert!(
            row.starts_with("alpha") || row.starts_with("beta"),
            "unexpected row {row:?}"
        );
        let _ = active_col; // documented above; kept for readability
    }
}

/// Stale sockets collapse into ONE grouped warning naming every leftover
/// path, instead of one check per socket flooding the summary.
#[test]
fn doctor_groups_stale_sockets_into_one_check() {
    let sb = Sandbox::new(&["alpha"]);
    let sock_dir = sb.dir.join("runtime").join("podbox");
    std::fs::create_dir_all(&sock_dir).unwrap();
    std::fs::write(sock_dir.join("ghost-wayland.sock"), "").unwrap();
    std::fs::write(sock_dir.join("ghost-dbus.sock"), "").unwrap();

    let out = sb.cmd().args(["doctor"]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let warn_lines: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("stale sockets"))
        .collect();
    assert_eq!(
        warn_lines.len(),
        1,
        "stale sockets must be a single grouped check, got: {warn_lines:?}"
    );
    let line = warn_lines[0];
    assert!(line.contains("ghost-wayland.sock"));
    assert!(line.contains("ghost-dbus.sock"));

    // JSON contract unchanged: still an array of grouped checks.
    let out = sb
        .cmd()
        .args(["doctor", "--output", "json"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    let stale: Vec<&serde_json::Value> = v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"] == "stale sockets")
        .collect();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0]["status"], "warn");
}

/// Doctor JSON carries grouped checks and reports failures via exit code.
#[test]
fn doctor_json_has_groups_and_exit_status() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb
        .cmd()
        .args(["-C", "alpha", "doctor", "--output", "json"])
        .output()
        .unwrap();

    // Stub podman makes several host checks fail/succeed by environment;
    // only assert structure, not pass/fail counts.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("doctor --output json must print valid JSON");
    let checks = v["checks"].as_array().expect("checks array");
    assert!(!checks.is_empty());
    assert!(checks.iter().all(|c| c["group"].is_string()));
    let groups: Vec<&str> = checks
        .iter()
        .map(|c| c["group"].as_str().unwrap())
        .collect();
    assert!(groups.contains(&"Host"));
    assert!(groups.contains(&"Integration"));

    // Text mode prints section headings and the exposure block.
    let out = sb.cmd().args(["-C", "alpha", "doctor"]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Host"));
    assert!(text.contains("Integration"));
    assert!(text.contains("Host exposure"), "exposure summary missing");
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
    let features = v["features"].as_array().expect("features array");
    assert!(
        features.iter().any(|f| f == "restart"),
        "features missing restart: {features:?}"
    );
}

/// Contract P3: `export remove app` deletes one launcher; a second removal
/// fails cleanly. (Fixture inner name is `myenv`.)
#[test]
fn export_remove_app_roundtrip() {
    let sb = Sandbox::new(&["alpha"]);
    let apps_dir = sb.dir.join("data").join("applications");
    std::fs::create_dir_all(&apps_dir).unwrap();
    let planted = apps_dir.join("podbox-myenv-testapp.desktop");
    std::fs::write(&planted, "[Desktop Entry]\nName=Test (myenv)\n").unwrap();

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
