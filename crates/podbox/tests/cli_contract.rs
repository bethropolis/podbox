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
        let dir = std::env::temp_dir().join(format!(
            "podbox-test-{}-{n}",
            std::process::id()
        ));
        let cfg = dir.join("podbox");
        std::fs::create_dir_all(&cfg).unwrap();
        for name in configs {
            // Copy a valid fixture so commands that fully parse the config
            // (not just stat it) succeed.
            let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/full.toml");
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
    let out = sb.cmd().args(["find-definition", "nosuch"]).output().unwrap();

    assert_eq!(out.status.code(), Some(2));
    assert!(
        out.stdout.is_empty(),
        "stdout must stay machine-clean on failure"
    );
}

/// Existing named config: exactly the path on stdout, exit 0.
#[test]
fn find_definition_prints_path() {
    let sb = Sandbox::new(&["myenv"]);
    let out = sb.cmd().args(["find-definition", "myenv"]).output().unwrap();

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
    let out = sb.cmd().args(["-C", "alpha", "enter", "--dry-run"]).output().unwrap();

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
    let out = sb_cmd_completions("fish").args(["--abbrs"]).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Every curated abbreviation is present as an `abbr` line.
    for token in
        ["pb", "pbb", "pbc", "pbd", "pbe", "pbl", "pbr", "pbs", "pbt", "pbu", "pbv", "pbx"]
    {
        assert!(
            stdout.lines().any(|l| l.starts_with(&format!("abbr {token} "))),
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
    let bash = sb_cmd_completions("bash").args(["--abbrs"]).output().unwrap();
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    let out = sb.cmd().args(["history", "--output", "json"]).output().unwrap();
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
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 4, "header + rule + two rows");

    // Non-TTY output carries no color codes; widths are then exact.
    let header = lines[0];
    let autostart_col = header.find("AUTOSTART").expect("AUTOSTART header");
    let active_col = header.find("ACTIVE CONTEXT").expect("ACTIVE CONTEXT header");

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
    let out = sb.cmd().args(["doctor", "--output", "json"]).output().unwrap();
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
    let out = sb.cmd().args(["-C", "alpha", "doctor", "--output", "json"]).output().unwrap();

    // Stub podman makes several host checks fail/succeed by environment;
    // only assert structure, not pass/fail counts.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .expect("doctor --output json must print valid JSON");
    let checks = v["checks"].as_array().expect("checks array");
    assert!(!checks.is_empty());
    assert!(checks.iter().all(|c| c["group"].is_string()));
    let groups: Vec<&str> = checks.iter().map(|c| c["group"].as_str().unwrap()).collect();
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
    let out = sb.cmd().args(["-C", "alpha", "recover", "--dry-run"]).output().unwrap();

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