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
        ("zsh", "__complete-names"),
    ] {
        let out = sb_cmd_completions(shell).output().unwrap();
        assert!(out.status.success(), "{shell}");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains(marker),
            "{shell} script should reference {marker}"
        );
    }
}

fn sb_cmd_completions(shell: &str) -> Command {
    let mut c = Sandbox::new(&[]).cmd();
    c.args(["completions", shell]);
    c
}