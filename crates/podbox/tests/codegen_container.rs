//! Quadlet `.container` unit: core directives
//! Split out of the former monolithic `codegen_tests.rs`.

mod common;

use common::*;

use podbox::codegen::quadlet;
use std::path::PathBuf;

#[test]
fn quadlet_container_has_userns_custom() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("UserNS=nomap"));
}
#[test]
fn quadlet_container_userns_defaults_to_keep_id() {
    let config = load_config("minimal.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("UserNS=keep-id"));
}
#[test]
fn quadlet_container_has_read_only_rootfs() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("ReadOnly=true"));
}
#[test]
fn quadlet_container_has_cpu_quota() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("CpuQuota="));
    // full.toml has cpus = "4.0" → 400000
    assert!(q.contains("CpuQuota=400000"));
}

#[test]
fn quadlet_emits_offline_scheduling_cache_and_service_policy() {
    let mut config = load_config("minimal.toml");
    config.network.offline = true;
    config.network.ports = vec!["8080:80".into()];
    config.container.cpu_weight = 260;
    config.storage.shared_caches.cargo = true;
    config.container.services.insert(
        "redis".into(),
        podbox::config::ServiceConfig::Short("redis-server".into()),
    );
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Network=none"));
    assert!(!q.contains("PublishPort="));
    assert!(q.contains("Slice=podbox.slice"));
    assert!(q.contains("CPUWeight=260"));
    assert!(q.contains("Volume=podbox-cache-cargo-registry:/home/%u/.cargo/registry:U"));
    assert!(q.contains("Volume=podbox-cache-cargo-git:/home/%u/.cargo/git:U"));
    assert!(!q.contains("Volume=podbox-cache-cargo:/home/%u/.cargo:U"));
    assert!(q.contains("Environment=PODBOX_SERVICES_JSON="));
}

#[test]
fn quadlet_host_cache_mbx_is_a_bind_mount_without_userns_flag() {
    let mut config = load_config("minimal.toml");
    config.storage.host_caches.mbx = true;
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Volume=%h/.cache/mbx:/home/%u/.cache/mbx:rw,z"));
    // A host directory is already owned by the host user, so the `:U`
    // ownership remap that named volumes need would be wrong here.
    assert!(!q.contains(".cache/mbx:U"));
}

#[test]
fn quadlet_host_cache_custom_expands_both_sides() {
    let mut config = load_config("minimal.toml");
    config
        .storage
        .host_caches
        .custom
        .push(podbox::config::HostCacheConfig {
            name: "zig".into(),
            host_path: "~/.cache/zig".into(),
            container_path: "~/.cache/zig".into(),
        });
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Volume=%h/.cache/zig:/home/%u/.cache/zig:rw,z"));
}

#[test]
fn quadlet_emits_no_cache_volumes_unless_configured() {
    let config = load_config("minimal.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(!q.contains("podbox-cache-"));
    assert!(!q.contains(".cache/mbx"));
}

#[test]
fn quadlet_escapes_user_environment_systemd_specifiers() {
    let mut config = load_config("minimal.toml");
    config
        .container
        .env
        .values
        .insert("FORMAT".into(), "date +%s".into());
    config.container.services.insert(
        "clock".into(),
        podbox::config::ServiceConfig::Short("date +%s".into()),
    );

    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Environment=FORMAT=\"date +%%s\""));
    assert!(q.contains("date +%%s"));
    // Podbox's own systemd specifier must remain active, not be escaped.
    assert!(q.contains("Environment=HOST_UID=%U"));
}

#[test]
fn quadlet_container_has_security_label_disable() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("SecurityLabelDisable=true"));
}
#[test]
fn quadlet_container_has_init() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("PodmanArgs=--init"));
}
#[test]
fn quadlet_container_has_network_host() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Network=host"));
    // host mode should not emit PublishPort
    assert!(!q.contains("PublishPort"));
}
#[test]
fn quadlet_no_host_home_mount() {
    let _guard = HOME_LOCK.lock().unwrap();
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    let home = dirs::home_dir().unwrap();
    let home_str = home.to_string_lossy();
    // Host home alone must never appear as Volume source
    assert!(!q.contains(&format!("{home_str}:")));
    // Expanded config.home path is used
    assert!(q.contains(&format!("Volume={home_str}/containers/myenv:/home/%u:Z")));
}
#[test]
fn quadlet_has_host_guest_socket_volume() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Volume=%t/podbox/myenv.sock:%t/podbox/myenv.sock"));
}
#[test]
fn quadlet_has_extra_env() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Environment=EDITOR=nvim"));
    assert!(q.contains("Environment=TERM=xterm-256color"));
}
#[test]
fn quadlet_has_extra_mounts() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Volume=~/Work:/home/user/Work:z"));
}
#[test]
fn quadlet_socket_file_has_listen_stream() {
    let config = load_config("full.toml");
    let q = quadlet::generate_socket(&config);
    assert!(q.contains("ListenStream=%t/podbox/myenv.sock"));
    assert!(q.contains("SocketMode=0600"));
    assert!(q.contains("RuntimeDirectoryPreserve=yes"));
}
#[test]
fn quadlet_build_file_has_image_tag() {
    let config = load_config("full.toml");
    let cf_path = PathBuf::from("/home/user/.local/share/podbox/myenv/Containerfile");
    let q = quadlet::generate_build(&config, &cf_path);
    assert!(q.contains("ImageTag=localhost/podbox-myenv:latest"));
    assert!(q.contains("File=/home/user/.local/share/podbox/myenv/Containerfile"));
}
#[test]
fn quadlet_uses_literal_percent_t() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("%t"));
    // %t must NOT be substituted
    assert!(!q.contains("/run/user/1000"));
}
#[test]
fn quadlet_no_literal_percent_h() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    // All home paths use expanded config values, not %h
    assert!(!q.contains("%h"));
}
#[test]
fn quadlet_auto_update_present_for_prebuilt() {
    let config = load_config("prebuilt.toml");
    let mut config = config.clone();
    config.lifecycle.auto_update = true;
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("AutoUpdate=registry"));
}
#[test]
fn quadlet_auto_update_present_for_build() {
    let config = load_config("full.toml");
    let mut config = config.clone();
    config.lifecycle.auto_update = true;
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("AutoUpdate=local"));
}
#[test]
fn quadlet_auto_update_absent_when_disabled() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(!q.contains("AutoUpdate"));
}
#[test]
fn quadlet_systemd_dependencies() {
    let config = load_config("full.toml");
    let mut config = config.clone();
    config.systemd.requires = vec!["db-container.service".into()];
    config.systemd.after = vec!["db-container.service".into()];
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Requires=db-container.service"));
    assert!(q.contains("After=db-container.service"));
}
#[test]
fn quadlet_systemd_dependencies_absent_by_default() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    let requires_lines: Vec<&str> = q.lines().filter(|l| l.starts_with("Requires=")).collect();
    // Socket + D-Bus proxy service (portal preset) + compositor service (wayland default)
    assert_eq!(requires_lines.len(), 3);
    assert!(requires_lines.iter().any(|l| l.ends_with(".socket")));
    assert!(requires_lines.iter().any(|l| l.ends_with("-proxy.service")));
    assert!(
        requires_lines
            .iter()
            .any(|l| l.ends_with("-compositor.service"))
    );
}
#[test]
fn quadlet_container_has_restart_rate_limiting() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Restart=on-failure"));
    assert!(q.contains("RestartSec=2s"));
    assert!(q.contains("StartLimitBurst=5"));
    assert!(q.contains("StartLimitIntervalSec=30s"));
}
