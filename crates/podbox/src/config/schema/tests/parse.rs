//! Parsing, defaults and top-level config surface

use super::super::*;
use crate::config::GpuMode;
use crate::config::OnStop;

#[test]
fn test_from_str_minimal() {
    let toml = r#"
[image]
base = "fedora:41"
name = "myenv"

[container]
name = "myenv"
home = "~/containers/myenv"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert_eq!(cfg.image.base, "fedora:41");
    assert_eq!(cfg.image.name, "myenv");
    assert_eq!(cfg.container.name, "myenv");
    assert_eq!(cfg.container.shell, "bash");
    assert_eq!(cfg.integration.gpu, GpuMode::Auto);
    assert!(!cfg.integration.wayland);
    assert!(!cfg.integration.audio);
    assert!(!cfg.integration.dbus);
    assert!(!cfg.integration.notify);
    assert!(!cfg.integration.xdg_open.enabled);
    assert!(cfg.integration.xdg_open.allowed_schemes.is_empty());
    assert!(!cfg.integration.clipboard);
    assert!(!cfg.integration.host_exec.enabled);
    assert!(cfg.integration.host_exec.allowlist.is_none());
    assert!(!cfg.integration.ssh_agent);
    assert!(!cfg.integration.git_identity);
    assert!(!cfg.lifecycle.auto_checkpoint);
}

#[test]
fn container_environment_supports_static_and_forwarded_values() {
    let cfg = Config::parse(
        r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[container.env]
EDITOR = "nvim"
forward = ["SSH_AUTH_SOCK", "AWS_*"]
"#,
    )
    .unwrap();
    assert_eq!(
        cfg.container.env.values.get("EDITOR").map(String::as_str),
        Some("nvim")
    );
    assert_eq!(cfg.container.env.forward, vec!["SSH_AUTH_SOCK", "AWS_*"]);
}

#[test]
fn cache_and_service_definitions_parse_with_opt_in_defaults() {
    let cfg = Config::parse(
        r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[container.services]
redis = "redis-server"
postgres = { command = "postgres", restart = "always", env = { PGDATA = "/data" } }
[storage.shared_caches]
cargo = true
uv = true
yarn = true
bun = true
composer = true
maven = true
gradle = true
"#,
    )
    .unwrap();
    assert!(cfg.storage.shared_caches.builtins.cargo);
    assert!(cfg.storage.shared_caches.builtins.uv);
    assert!(cfg.storage.shared_caches.builtins.yarn);
    assert!(cfg.storage.shared_caches.builtins.bun);
    assert!(cfg.storage.shared_caches.builtins.composer);
    assert!(!cfg.storage.shared_caches.builtins.npm);
    assert!(cfg.storage.shared_caches.builtins.maven);
    assert!(cfg.storage.shared_caches.builtins.gradle);
    assert_eq!(cfg.container.services.len(), 2);
}

#[test]
fn host_caches_are_opt_in_and_default_empty() {
    let cfg = Config::parse(
        r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#,
    )
    .unwrap();
    assert!(!cfg.storage.host_caches.builtins.mbx);
    assert!(cfg.storage.host_caches.custom.is_empty());
    assert!(cfg.storage.host_caches.is_empty());
}

#[test]
fn host_caches_parse_with_mbx_and_custom_entries() {
    let cfg = Config::parse(
        r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[storage.host_caches]
mbx = true
[[storage.host_caches.custom]]
name = "zig"
host_path = "~/.cache/zig"
container_path = "~/.cache/zig"
"#,
    )
    .unwrap();
    assert!(cfg.storage.host_caches.builtins.mbx);
    assert_eq!(cfg.storage.host_caches.custom.len(), 1);
    assert_eq!(cfg.storage.host_caches.custom[0].host_path, "~/.cache/zig");
}

#[test]
fn host_caches_survive_a_serialization_round_trip() {
    let cfg = Config::parse(
        r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[storage.host_caches]
mbx = true
"#,
    )
    .unwrap();
    let serialized = toml::to_string_pretty(&cfg).unwrap();
    assert!(serialized.contains("host_caches"), "{serialized}");
    let reparsed = Config::parse(&serialized).unwrap();
    assert!(reparsed.storage.host_caches.builtins.mbx);
}
#[test]
fn test_home_tilde_expanded() {
    let toml = r#"
[image]
base = "fedora:41"
name = "myenv"

[container]
name = "myenv"
home = "~/containers/myenv"
"#;
    let cfg = Config::parse(toml).unwrap();
    let home = dirs::home_dir().unwrap();
    assert!(cfg.container.home.starts_with(&home));
    assert!(
        cfg.container
            .home
            .to_string_lossy()
            .contains("containers/myenv")
    );
}
#[test]
fn test_on_stop_defaults_to_keep() {
    let toml = r#"
[image]
base = "fedora:41"
name = "myenv"

[container]
name = "myenv"
home = "~/containers/myenv"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert_eq!(cfg.lifecycle.on_stop, OnStop::Keep);
}
#[test]
fn test_xdg_dirs_default_all_false() {
    let toml = r#"
[image]
base = "fedora:41"
name = "myenv"

[container]
name = "myenv"
home = "~/containers/myenv"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert!(!cfg.integration.xdg_dirs.documents.is_enabled());
    assert!(!cfg.integration.xdg_dirs.downloads.is_enabled());
    assert!(!cfg.integration.xdg_dirs.pictures.is_enabled());
    assert!(!cfg.integration.xdg_dirs.music.is_enabled());
    assert!(!cfg.integration.xdg_dirs.videos.is_enabled());
    assert!(!cfg.integration.xdg_dirs.desktop.is_enabled());
}
#[test]
fn test_integration_defaults_to_opt_in() {
    let toml = r#"
[image]
base = "fedora:41"
name = "myenv"

[container]
name = "myenv"
home = "~/containers/myenv"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert!(!cfg.integration.git_identity);
    assert!(!cfg.integration.wayland);
    assert!(!cfg.integration.audio);
    assert!(!cfg.integration.dbus);
}
#[test]
fn test_embedded_default_parses() {
    let cfg = Config::embedded();
    assert_eq!(cfg.image.base, "fedora:44");
    assert_eq!(cfg.image.name, "podbox");
    assert_eq!(cfg.container.name, "podbox");
    assert!(!cfg.integration.wayland);
    assert!(!cfg.integration.audio);
    assert!(!cfg.integration.dbus);
    assert_eq!(cfg.integration.gpu, GpuMode::Auto);
    assert!(!cfg.lifecycle.quadlet);
}
#[test]
fn test_config_load_not_found() {
    let path = std::path::Path::new("/tmp/does_not_exist_XXXXX.toml");
    let result = Config::load(path);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.downcast_ref::<PodboxError>().is_some());
}
#[test]
fn test_systemd_config_parses() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[systemd]
requires = ["db.service", "cache.service"]
after = ["network.target"]
"#;
    let cfg = Config::parse(toml).unwrap();
    assert_eq!(cfg.systemd.requires, vec!["db.service", "cache.service"]);
    assert_eq!(cfg.systemd.after, vec!["network.target"]);
}
#[test]
fn test_visual_config_parses() {
    let toml = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
[integration]
sync_themes = true
sync_icons = true
sync_fonts = true
"#;
    let cfg = Config::parse(toml).unwrap();
    assert!(cfg.integration.sync_themes);
    assert!(cfg.integration.sync_icons);
    assert!(cfg.integration.sync_fonts);
}
#[test]
fn test_invalid_toml_errors() {
    let toml = r#"
[image
base = "fedora:41"
"#;
    assert!(Config::parse(toml).is_err());
}
#[test]
fn test_missing_required_fields_errors() {
    let toml = r#"
[image]
base = "fedora:41"
"#;
    assert!(Config::parse(toml).is_err());
}

#[test]
fn provision_steps_parse_with_defaults() {
    let cfg = Config::parse(
        r#"
[image]
base = "fedora:44"
name = "env"
[container]
name = "env"
home = "~/env"

[[provision]]
name = "rust-toolchain"
run = "mise use -g rust@stable"

[[provision]]
name = "git-defaults"
run = "git config --global init.defaultBranch main"
on_failure = "abort"
env = { FOO = "bar" }
root = false
timeout = "10m"
"#,
    )
    .unwrap();
    assert_eq!(cfg.provision.len(), 2);
    assert_eq!(cfg.provision[0].name, "rust-toolchain");
    assert_eq!(cfg.provision[0].run, "mise use -g rust@stable");
    assert_eq!(cfg.provision[0].timeout, "5m");
    assert_eq!(
        cfg.provision[0].on_failure,
        crate::config::types::ProvisionOnFailure::Warn
    );
    assert!(cfg.provision[0].env.is_empty());
    assert!(!cfg.provision[0].root);
    assert_eq!(cfg.provision[1].timeout, "10m");
    assert_eq!(
        cfg.provision[1].on_failure,
        crate::config::types::ProvisionOnFailure::Abort
    );
    assert_eq!(
        cfg.provision[1].env.get("FOO").map(String::as_str),
        Some("bar")
    );
}

#[test]
fn empty_provision_is_omitted_from_serialized_toml() {
    let cfg = Config::parse(
        r#"
[image]
base = "fedora:44"
name = "env"
[container]
name = "env"
home = "~/env"
"#,
    )
    .unwrap();
    assert!(cfg.provision.is_empty());
    let out = toml::to_string_pretty(&cfg).unwrap();
    assert!(!out.contains("provision"), "{out}");
}

#[test]
fn provision_steps_round_trip_through_toml() {
    let cfg = Config::parse(
        r#"
[image]
base = "fedora:44"
name = "env"
[container]
name = "env"
home = "~/env"

[[provision]]
name = "toolchain"
run = "mise use -g rust@stable"
timeout = "1h"
"#,
    )
    .unwrap();
    let serialized = toml::to_string_pretty(&cfg).unwrap();
    assert!(serialized.contains("[[provision]]"), "{serialized}");
    let reparsed = Config::parse(&serialized).unwrap();
    assert_eq!(reparsed.provision.len(), 1);
    assert_eq!(reparsed.provision[0].name, "toolchain");
    assert_eq!(reparsed.provision[0].run, "mise use -g rust@stable");
    assert_eq!(reparsed.provision[0].timeout, "1h");
}

#[test]
fn provision_run_accepts_multi_line_scripts() {
    // `run` goes straight to `sh -c`, so embedded newlines are a script,
    // not a syntax error. Triple-quoted TOML carries them.
    let cfg = Config::parse(
        r#"
[image]
base = "fedora:44"
name = "env"
[container]
name = "env"
home = "~/env"

[[provision]]
name = "toolchain"
run = """
set -e
mise use -g rust@stable
"""
"#,
    )
    .unwrap();
    assert_eq!(cfg.provision[0].run, "set -e\nmise use -g rust@stable\n");
    // Round-trips byte-identical through serialization.
    let serialized = toml::to_string_pretty(&cfg).unwrap();
    let reparsed = Config::parse(&serialized).unwrap();
    assert_eq!(reparsed.provision[0].run, cfg.provision[0].run);
}
