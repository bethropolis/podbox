//! Golden shape tests for the client contract (`docs/clients.md`).
//!
//! Every JSON reader's output shape is pinned against a fixture in
//! `fixtures/golden/`. CI fails when a shape changes: adding, removing, or
//! retyping a field breaks the comparison until the fixture is updated
//! deliberately. Clients can copy the fixtures to build their own tests.
//!
//! Comparison rules (values are ignored unless noted):
//! - Objects: exact key sets after stripping documented volatile paths.
//! - Arrays: empty live arrays pass (the sandbox lacks data); scalar arrays
//!   compare exactly (the `info` feature ledger); object arrays compare each
//!   live item against the fixture's first item.
//! - Scalars: kinds must match, except `null` which matches anything
//!   (optional fields resolve or not depending on environment).
//! - Stripped as volatile: `status` enrichment (running-only fields) and
//!   `inspect.env` host-conditional keys. `diff` is excluded: it needs a
//!   running container, which the sandbox stub cannot provide.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use assert_cmd::prelude::*;
use serde_json::Value;

struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(configs: &[&str]) -> Self {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("podbox-golden-{n}",));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = dir.join("podbox");
        std::fs::create_dir_all(&cfg).unwrap();
        for name in configs {
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
        std::fs::set_permissions(&stub, PermissionsExt::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("podbox").unwrap();
        c.env("XDG_CONFIG_HOME", &self.dir)
            .env("XDG_DATA_HOME", self.dir.join("data"))
            .env("XDG_STATE_HOME", self.dir.join("state"))
            .env("XDG_RUNTIME_DIR", self.dir.join("runtime"))
            .env_remove("PODBOX_CONTAINER")
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.dir.join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        c
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Remove a dotted path (e.g. `env.wayland_display`) from both trees.
fn strip(value: &mut Value, path: &str) {
    let mut parts: Vec<&str> = path.split('.').collect();
    let Some(last) = parts.pop() else { return };
    let mut current = value;
    for part in parts {
        match current.get_mut(part) {
            Some(next) => current = next,
            None => return,
        }
    }
    if let Some(obj) = current.as_object_mut() {
        obj.remove(last);
    }
}

fn compare_shape(fixture: &Value, live: &Value, path: &str) -> Result<(), String> {
    match (fixture, live) {
        (Value::Object(f), Value::Object(l)) => {
            let mut fkeys: Vec<&String> = f.keys().collect();
            let mut lkeys: Vec<&String> = l.keys().collect();
            fkeys.sort();
            lkeys.sort();
            if fkeys != lkeys {
                return Err(format!(
                    "{path}: keys differ (fixture {fkeys:?}, live {lkeys:?})"
                ));
            }
            for key in fkeys {
                compare_shape(&f[key], &l[key], &format!("{path}.{key}"))?;
            }
            Ok(())
        }
        (Value::Array(f), Value::Array(l)) => {
            if l.is_empty() {
                return Ok(());
            }
            let Some(example) = f.first() else {
                return Err(format!(
                    "{path}: live has items but the fixture array is empty"
                ));
            };
            if example.is_string() || example.is_number() || example.is_boolean() {
                if fixture != live {
                    return Err(format!("{path}: scalar array differs"));
                }
                return Ok(());
            }
            for (i, item) in l.iter().enumerate() {
                compare_shape(example, item, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        _ => {
            if kind(fixture) == kind(live) || kind(fixture) == "null" || kind(live) == "null" {
                Ok(())
            } else {
                Err(format!(
                    "{path}: kind changed (fixture {}, live {})",
                    kind(fixture),
                    kind(live)
                ))
            }
        }
    }
}

/// Run `args`, parse stdout as one JSON document, and pin its shape.
fn assert_golden(sb: &Sandbox, args: &[&str], fixture: &str, strips: &[&str]) {
    let out = sb.cmd().args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut live: Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim())
        .unwrap_or_else(|_| panic!("{args:?} must print a single JSON document"));
    let path = format!("fixtures/golden/{fixture}");
    let raw = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(&path),
    )
    .unwrap_or_else(|_| panic!("missing fixture {path}"));
    let mut expected: Value = serde_json::from_str(&raw).unwrap();
    for strip_path in strips {
        strip(&mut live, strip_path);
        strip(&mut expected, strip_path);
    }
    compare_shape(&expected, &live, "$")
        .unwrap_or_else(|mismatch| panic!("{args:?} shape changed ({mismatch}); update {path}"));
}

#[test]
fn golden_list_empty() {
    let sb = Sandbox::new(&[]);
    assert_golden(&sb, &["list", "--output", "json"], "list_empty.json", &[]);
}

#[test]
fn golden_list_one() {
    let sb = Sandbox::new(&["alpha"]);
    assert_golden(&sb, &["list", "--output", "json"], "list_one.json", &[]);
}

#[test]
fn golden_status() {
    let sb = Sandbox::new(&["alpha"]);
    assert_golden(
        &sb,
        &["-C", "alpha", "status", "--output", "json"],
        "status.json",
        &[
            "started_at",
            "cpu_percent",
            "memory_bytes",
            "memory_limit_bytes",
            "pids",
        ],
    );
}

#[test]
fn golden_history() {
    let sb = Sandbox::new(&["alpha"]);
    assert_golden(&sb, &["history", "--output", "json"], "history.json", &[]);
}

#[test]
fn golden_snapshot_list() {
    let sb = Sandbox::new(&["alpha"]);
    assert_golden(
        &sb,
        &["-C", "alpha", "snapshot", "list", "--output", "json"],
        "snapshot_list.json",
        &[],
    );
}

#[test]
fn golden_provision_status() {
    let sb = Sandbox::new(&["alpha"]);
    let prov = sb.dir.join("podbox").join("prov.toml");
    let mut text = std::fs::read_to_string(sb.dir.join("podbox").join("alpha.toml")).unwrap();
    text.push_str("\n[[provision]]\nname = \"toolchain\"\nrun = \"true\"\n");
    std::fs::write(&prov, &text).unwrap();
    assert_golden(
        &sb,
        &["-C", "prov", "provision", "status", "--output", "json"],
        "provision_status.json",
        &[],
    );
}

#[test]
fn golden_inspect() {
    let sb = Sandbox::new(&["alpha"]);
    assert_golden(
        &sb,
        &["-C", "alpha", "inspect", "--output", "json"],
        "inspect.json",
        &[
            "env.wayland_display",
            "env.dbus_socket",
            "env.gpg_agent",
            "env.host_shell",
            "env.host_locale",
        ],
    );
}

#[test]
fn golden_doctor() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb
        .cmd()
        .args(["-C", "alpha", "doctor", "--output", "json"])
        .output()
        .unwrap();
    let live: Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim())
        .expect("doctor must print a single JSON document");
    let raw = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden/doctor.json"),
    )
    .unwrap();
    let expected: Value = serde_json::from_str(&raw).unwrap();
    compare_shape(&expected, &live, "$")
        .unwrap_or_else(|mismatch| panic!("doctor shape changed ({mismatch})"));
}

#[test]
fn golden_export_list() {
    let sb = Sandbox::new(&["alpha"]);
    assert_golden(
        &sb,
        &["-C", "alpha", "export", "list", "--output", "json"],
        "export_list.json",
        &[],
    );
}

#[test]
fn golden_info() {
    let sb = Sandbox::new(&[]);
    assert_golden(&sb, &["info", "--output", "json"], "info.json", &[]);
}

#[test]
fn golden_config_get() {
    let sb = Sandbox::new(&["alpha"]);
    assert_golden(
        &sb,
        &[
            "-C",
            "alpha",
            "config",
            "get",
            "container.shell",
            "--output",
            "json",
        ],
        "config_get.json",
        &[],
    );
}

#[test]
fn golden_config_validate() {
    let sb = Sandbox::new(&["alpha"]);
    assert_golden(
        &sb,
        &["-C", "alpha", "config", "validate", "--output", "json"],
        "config_validate.json",
        &[],
    );
}

#[test]
fn golden_error_envelope() {
    let sb = Sandbox::new(&[]);
    let out = sb
        .cmd()
        .args(["-C", "nosuch", "status", "--output", "json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let live: Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim())
        .expect("error mode must print a single JSON document");
    let raw = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden/error.json"),
    )
    .unwrap();
    let expected: Value = serde_json::from_str(&raw).unwrap();
    compare_shape(&expected, &live, "$")
        .unwrap_or_else(|mismatch| panic!("error shape changed ({mismatch})"));
}
