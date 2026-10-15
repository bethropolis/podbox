//! Unit-value escaping: injection resistance and specifier handling.
//!
//! Values reaching generated units go through a single writer that rejects
//! control characters and doubles `%`; validation reports violations with
//! field paths before codegen runs.

mod common;

use common::*;
use std::path::PathBuf;

use podbox::codegen::quadlet;
use podbox::config::Config;

#[test]
fn unit_dependency_with_newline_is_a_validation_error() {
    let err = Config::parse(
        r#"
[image]
base = "fedora:41"
name = "minimal"

[container]
name = "minimal"
home = "~/containers/minimal"

[systemd]
after = ["x\nExecStartPre=/bin/evil"]
"#,
    )
    .expect_err("newline in systemd.after must fail validation");
    assert!(
        err.to_string().contains("systemd.after[0]"),
        "error must name the field path: {err}"
    );
}

#[test]
fn percent_in_home_path_is_doubled_in_volume() {
    let mut config = load_config("minimal.toml");
    config.container.home = PathBuf::from("/srv/%h-data");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(
        q.contains("Volume=/srv/%%h-data:/home/%u:Z"),
        "literal % must be doubled so systemd keeps it: \n{q}"
    );
}

#[test]
fn env_value_with_tab_and_quote_round_trips() {
    let mut config = load_config("minimal.toml");
    config
        .container
        .env
        .values
        .insert("TRICKY".into(), "a\tb\"c".into());
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    let line = q
        .lines()
        .find(|l| l.starts_with("Environment=TRICKY="))
        .expect("TRICKY assignment must be emitted");
    // Podman word-splitting unescape: inside quotes, \X collapses to X.
    let encoded = line.strip_prefix("Environment=TRICKY=").unwrap();
    let mut decoded = String::new();
    let mut chars = encoded.chars();
    assert_eq!(chars.next(), Some('"'), "must be quoted: {line}");
    while let Some(c) = chars.next() {
        if c == '"' {
            break;
        }
        if c == '\\' {
            match chars.next() {
                Some('t') => decoded.push('\t'),
                Some(other) => decoded.push(other),
                None => decoded.push('\\'),
            }
        } else {
            decoded.push(c);
        }
    }
    assert_eq!(decoded, "a\tb\"c", "round-trip failed: {line}");
}

#[test]
fn multiline_run_command_is_a_validation_error() {
    let err = Config::parse(
        r#"
[image]
base = "fedora:41"
name = "minimal"

[image.run]
commands = ["echo one\necho two"]

[container]
name = "minimal"
home = "~/containers/minimal"
"#,
    )
    .expect_err("multi-line run command must fail validation");
    assert!(
        err.to_string().contains("image.run.commands[0]"),
        "error must name the field path: {err}"
    );
}

#[test]
fn no_raw_format_pushes_remain_outside_the_writer() {
    // The single-writer rule, enforced structurally: the only
    // `lines.push(format!` in quadlet codegen lives in value.rs itself.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/codegen/quadlet");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let content = std::fs::read_to_string(&path).unwrap();
        if path.file_name().unwrap() == "value.rs" {
            continue;
        }
        assert!(
            !content.contains("lines.push(format!"),
            "{} still pushes unescaped values",
            path.display()
        );
    }
}
