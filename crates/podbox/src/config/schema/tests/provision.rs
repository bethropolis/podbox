//! `[[provision]]` validation rules

use super::super::*;
use crate::config::types::ProvisionOnFailure;

fn base(steps: &str) -> String {
    format!(
        r#"
[image]
base = "fedora:44"
name = "env"

[container]
name = "env"
home = "~/env"
{steps}
"#
    )
}

#[test]
fn valid_steps_pass() {
    let toml = base(
        r#"
[[provision]]
name = "toolchain"
run = "mise use -g rust@stable"
env = { RUST_VERSION = "stable", _ALLOWED = "yes" }
"#,
    );
    Config::parse(&toml).unwrap();
}

#[test]
fn reserved_and_invalid_names_are_rejected() {
    let err = Config::parse(&base(
        r#"
[[provision]]
name = "dotfiles"
run = "echo hi"
"#,
    ))
    .unwrap_err()
    .to_string();
    assert!(err.contains("reserved"), "{err}");

    for bad in ["-leading", "UPPER", "has space", "", "a.b"] {
        let toml = base(&format!(
            r#"
[[provision]]
name = "{bad}"
run = "echo hi"
"#
        ));
        let err = Config::parse(&toml).unwrap_err().to_string();
        assert!(err.contains("provision[0].name"), "{bad}: {err}");
    }
    // 64 chars is the documented maximum.
    let long = "a".repeat(64);
    Config::parse(&base(&format!(
        r#"
[[provision]]
name = "{long}"
run = "echo hi"
"#
    )))
    .unwrap();
    let too_long = "a".repeat(65);
    let err = Config::parse(&base(&format!(
        r#"
[[provision]]
name = "{too_long}"
run = "echo hi"
"#
    )))
    .unwrap_err()
    .to_string();
    assert!(err.contains("provision[0].name"), "{err}");
}

#[test]
fn duplicate_names_are_rejected() {
    let err = Config::parse(&base(
        r#"
[[provision]]
name = "same"
run = "echo one"

[[provision]]
name = "same"
run = "echo two"
"#,
    ))
    .unwrap_err()
    .to_string();
    assert!(err.contains("duplicate"), "{err}");
}

#[test]
fn empty_run_is_rejected() {
    let err = Config::parse(&base(
        r#"
[[provision]]
name = "noop"
run = "   "
"#,
    ))
    .unwrap_err()
    .to_string();
    assert!(err.contains("provision[0].run"), "{err}");
}

#[test]
fn invalid_timeout_is_rejected() {
    // Trailing whitespace is trimmed (the parser trims), so not rejected.
    for bad in ["off", "5", "abc", "1x"] {
        let toml = base(&format!(
            r#"
[[provision]]
name = "step"
run = "echo hi"
timeout = "{bad}"
"#
        ));
        let err = Config::parse(&toml).unwrap_err().to_string();
        assert!(err.contains("provision[0].timeout"), "{bad}: {err}");
    }
    for good in ["1s", "30s", "5m", "1h"] {
        Config::parse(&base(&format!(
            r#"
[[provision]]
name = "step"
run = "echo hi"
timeout = "{good}"
"#
        )))
        .unwrap();
    }
    // Zero is explicitly not a duration.
    let err = Config::parse(&base(
        r#"
[[provision]]
name = "step"
run = "echo hi"
timeout = "0s"
"#,
    ))
    .unwrap_err()
    .to_string();
    assert!(err.contains("provision[0].timeout"), "{err}");
}

#[test]
fn env_key_rules_are_enforced() {
    // Podbox_ prefix is reserved (host-driven contract).
    let err = Config::parse(&base(
        r#"
[[provision]]
name = "step"
run = "echo hi"
env = { PODBOX_CONTAINER = "x" }
"#,
    ))
    .unwrap_err()
    .to_string();
    assert!(err.contains("reserved"), "{err}");
    // Leading digit is not a valid shell identifier.
    let err = Config::parse(&base(
        r#"
[[provision]]
name = "step"
run = "echo hi"
env = { "1BAD" = "x" }
"#,
    ))
    .unwrap_err()
    .to_string();
    assert!(err.contains("invalid key"), "{err}");
    // Newlines would break the exec environment.
    let err = Config::parse(&base(
        r#"
[[provision]]
name = "step"
run = "echo hi"
env = { K = "a\nb" }
"#,
    ))
    .unwrap_err()
    .to_string();
    assert!(err.contains("newline"), "{err}");
}

#[test]
fn too_many_steps_is_rejected() {
    let mut steps = String::new();
    for i in 0..65 {
        use std::fmt::Write as _;
        let _ = write!(
            steps,
            "[[provision]]\nname = \"s{i}\"\nrun = \"echo {i}\"\n\n"
        );
    }
    let err = Config::parse(&base(&steps)).unwrap_err().to_string();
    assert!(err.contains("too many steps"), "{err}");
}

#[test]
fn on_failure_parses_both_variants() {
    let cfg = Config::parse(&base(
        r#"
[[provision]]
name = "a"
run = "echo a"
on_failure = "warn"

[[provision]]
name = "b"
run = "echo b"
on_failure = "abort"
"#,
    ))
    .unwrap();
    assert_eq!(cfg.provision[0].on_failure, ProvisionOnFailure::Warn);
    assert_eq!(cfg.provision[1].on_failure, ProvisionOnFailure::Abort);
    let err = Config::parse(&base(
        r#"
[[provision]]
name = "a"
run = "echo a"
on_failure = "explode"
"#,
    ))
    .unwrap_err();
    // Serde's detail is wrapped in the "failed to parse definition file"
    // context, so assert the config was refused rather than a sub-message.
    assert!(err.to_string().contains("failed to parse"), "{err}");
}
