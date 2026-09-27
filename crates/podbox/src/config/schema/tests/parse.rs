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
    assert_eq!(cfg.container.shell, "fish");
    assert_eq!(cfg.integration.gpu, GpuMode::Auto);
    assert!(cfg.integration.wayland);
    assert!(cfg.integration.audio);
    assert!(cfg.integration.dbus);
    assert!(cfg.integration.notify);
    assert!(cfg.integration.xdg_open);
    assert!(cfg.integration.clipboard);
    assert!(!cfg.integration.host_exec.enabled);
    assert!(cfg.integration.host_exec.allowlist.is_none());
    assert!(!cfg.integration.ssh_agent);
    assert!(cfg.integration.git_identity);
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
"#,
    )
    .unwrap();
    assert!(cfg.storage.shared_caches.cargo);
    assert!(!cfg.storage.shared_caches.npm);
    assert_eq!(cfg.container.services.len(), 2);
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
fn test_wayland_default_is_true() {
    let toml = r#"
[image]
base = "fedora:41"
name = "myenv"

[container]
name = "myenv"
home = "~/containers/myenv"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert!(cfg.integration.wayland);
    assert!(cfg.integration.audio);
}
#[test]
fn test_embedded_default_parses() {
    let cfg = Config::embedded();
    assert_eq!(cfg.image.base, "fedora:44");
    assert_eq!(cfg.image.name, "podbox");
    assert_eq!(cfg.container.name, "podbox");
    assert!(cfg.integration.wayland);
    assert!(cfg.integration.audio);
    assert!(cfg.integration.dbus);
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
