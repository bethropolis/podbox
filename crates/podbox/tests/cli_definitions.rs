//! Definition discovery, non-TTY resolution, and shell completion contracts.

mod support;

use std::process::Command;

use support::Sandbox;

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
