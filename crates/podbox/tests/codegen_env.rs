//! Quadlet `.container` unit: env vars, mounts, XDG and theme sync
//! Split out of the former monolithic `codegen_tests.rs`.

mod common;

use common::*;

use podbox::codegen::quadlet;
use std::path::PathBuf;

#[test]
fn quadlet_ssh_agent_mounts_host_socket_when_present() {
    let mut config = load_config("full.toml");
    config.integration.ssh_agent = true;
    let env = env_with_ssh_agent(Some(PathBuf::from("/run/user/1000/keyring/ssh")));
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(q.contains("Volume=/run/user/1000/keyring/ssh:/run/podbox/ssh-agent.sock"));
    assert!(q.contains("Environment=SSH_AUTH_SOCK=/run/podbox/ssh-agent.sock"));
}
#[test]
fn quadlet_ssh_agent_absent_when_no_socket() {
    let mut config = load_config("full.toml");
    config.integration.ssh_agent = true;
    let env = env_with_ssh_agent(None);
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(!q.contains("ssh-agent.sock"));
    assert!(!q.contains("SSH_AUTH_SOCK"));
}
#[test]
fn quadlet_xdg_dir_present_when_enabled() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Volume=/home/user/Documents:/home/%u/Documents:ro,z"));
    assert!(q.contains("Volume=/home/user/Downloads:/home/%u/Downloads:ro,z"));
    assert!(q.contains("Environment=HOME=/home/%u"));
    assert!(q.contains("Environment=HOST_USER=testuser"));
    assert!(q.contains("Environment=HOST_UID=%U"));
    assert!(q.contains("Environment=HOST_GID=%G"));
}
#[test]
fn quadlet_xdg_dir_absent_when_disabled() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(!q.contains("Pictures"));
    assert!(!q.contains("Music"));
}
#[test]
fn quadlet_visual_themes_present() {
    with_sandbox_home(|home| {
        std::fs::create_dir_all(home.join(".local/share/themes")).unwrap();
        std::fs::create_dir_all(home.join(".themes")).unwrap();
        let config = load_config("full.toml");
        let mut config = config.clone();
        config.integration.sync_themes = true;
        let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
        assert!(q.contains("Volume=%h/.themes:/home/%u/.themes:ro"));
    });
}
#[test]
fn quadlet_visual_icons_present() {
    with_sandbox_home(|home| {
        std::fs::create_dir_all(home.join(".icons")).unwrap();
        let config = load_config("full.toml");
        let mut config = config.clone();
        config.integration.sync_icons = true;
        let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
        assert!(q.contains("Volume=%h/.icons:/home/%u/.icons:ro"));
    });
}
#[test]
fn quadlet_visual_fonts_present() {
    with_sandbox_home(|home| {
        std::fs::create_dir_all(home.join(".fonts")).unwrap();
        std::fs::create_dir_all(home.join(".config/fontconfig")).unwrap();
        let config = load_config("full.toml");
        let mut config = config.clone();
        config.integration.sync_fonts = true;
        let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
        assert!(q.contains("Volume=%h/.fonts:/home/%u/.fonts:ro"));
    });
}
#[test]
fn quadlet_visual_mounts_absent_by_default() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(!q.contains("Volume=%h/.themes"));
    assert!(!q.contains("Volume=%h/.icons"));
    assert!(!q.contains("Volume=%h/.fonts"));
    assert!(!q.contains("/home/%u/.themes"));
    assert!(!q.contains("/home/%u/.icons"));
    assert!(!q.contains("/home/%u/.fonts"));
}
#[test]
fn quadlet_timezone_mount_present_when_localtime_exists() {
    let config = load_config("full.toml");
    let mut env = default_env();
    env.host_has_localtime = true;
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(q.contains("Volume=/etc/localtime:/etc/localtime:ro"));
}
#[test]
fn quadlet_timezone_file_mount_present_when_exists() {
    let config = load_config("full.toml");
    let mut env = default_env();
    env.host_has_timezone_file = true;
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(q.contains("Volume=/etc/timezone:/etc/timezone:ro"));
}
#[test]
fn quadlet_timezone_mounts_absent_when_unavailable() {
    let config = load_config("full.toml");
    let env = default_env();
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(!q.contains("/etc/localtime"));
    assert!(!q.contains("/etc/timezone"));
}
#[test]
fn quadlet_modern_theme_path_present() {
    with_sandbox_home(|home| {
        std::fs::create_dir_all(home.join(".local/share/themes")).unwrap();
        std::fs::create_dir_all(home.join(".themes")).unwrap();
        let config = load_config("full.toml");
        let mut config = config.clone();
        config.integration.sync_themes = true;
        let mut env = default_env();
        env.host_has_local_share_themes = true;
        let q = quadlet::generate_container(&config, &env, &default_xdg());
        assert!(q.contains("Volume=%h/.themes:/home/%u/.themes:ro"));
        assert!(q.contains("Volume=%h/.local/share/themes:/home/%u/.local/share/themes:ro"));
    });
}
#[test]
fn quadlet_modern_icon_path_present() {
    with_sandbox_home(|home| {
        std::fs::create_dir_all(home.join(".icons")).unwrap();
        let config = load_config("full.toml");
        let mut config = config.clone();
        config.integration.sync_icons = true;
        let mut env = default_env();
        env.host_has_local_share_icons = true;
        let q = quadlet::generate_container(&config, &env, &default_xdg());
        assert!(q.contains("Volume=%h/.icons:/home/%u/.icons:ro"));
        assert!(q.contains("Volume=%h/.local/share/icons:/home/%u/.local/share/icons:ro"));
    });
}
#[test]
fn quadlet_modern_font_path_present() {
    with_sandbox_home(|home| {
        std::fs::create_dir_all(home.join(".fonts")).unwrap();
        std::fs::create_dir_all(home.join(".config/fontconfig")).unwrap();
        let config = load_config("full.toml");
        let mut config = config.clone();
        config.integration.sync_fonts = true;
        let mut env = default_env();
        env.host_has_local_share_fonts = true;
        let q = quadlet::generate_container(&config, &env, &default_xdg());
        assert!(q.contains("Volume=%h/.fonts:/home/%u/.fonts:ro"));
        assert!(q.contains("Volume=%h/.local/share/fonts:/home/%u/.local/share/fonts:ro"));
    });
}
#[test]
fn quadlet_modern_paths_absent_when_disabled() {
    let config = load_config("full.toml");
    let mut env = default_env();
    env.host_has_local_share_themes = true;
    env.host_has_local_share_icons = true;
    env.host_has_local_share_fonts = true;
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(!q.contains(".local/share/themes"));
    assert!(!q.contains(".local/share/icons"));
    assert!(!q.contains(".local/share/fonts"));
}
#[test]
fn quadlet_locale_env_vars_present() {
    let config = load_config("full.toml");
    let mut env = default_env();
    env.host_locale = Some("en_US.UTF-8".into());
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(q.contains("Environment=LANG=en_US.UTF-8"));
    assert!(q.contains("Environment=LC_ALL=en_US.UTF-8"));
    assert!(q.contains("Environment=LC_CTYPE=en_US.UTF-8"));
}
#[test]
fn quadlet_locale_env_vars_absent_when_unset() {
    let config = load_config("full.toml");
    let env = default_env();
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(!q.contains("Environment=LANG="));
    assert!(!q.contains("Environment=LC_ALL="));
}
