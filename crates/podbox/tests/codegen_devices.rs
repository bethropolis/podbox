//! Quadlet `.container` unit: GPU, Wayland and audio passthrough
//! Split out of the former monolithic `codegen_tests.rs`.

mod common;

use common::*;

use podbox::codegen::quadlet;
use podbox::config::GpuMode;

#[test]
fn quadlet_wayland_volume_present_when_enabled() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Volume=%t/podbox/myenv-wayland.sock:%t/wayland-0:ro"));
    assert!(q.contains("Environment=WAYLAND_DISPLAY=wayland-0"));
    assert!(q.contains("Environment=MOZ_ENABLE_WAYLAND=1"));
    assert!(q.contains("Requires=myenv-compositor.service"));
    assert!(q.contains("After=myenv-compositor.service"));
}
#[test]
fn quadlet_wayland_absent_when_disabled() {
    let config = load_config("no_wayland.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(!q.contains("wayland-0"));
    assert!(!q.contains("WAYLAND_DISPLAY"));
    assert!(!q.contains("MOZ_ENABLE_WAYLAND"));
}
#[test]
fn quadlet_audio_volumes_present() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Volume=%t/pipewire-0:%t/pipewire-0"));
    assert!(q.contains("Volume=%t/pulse:%t/pulse"));
    assert!(q.contains("Environment=PULSE_SERVER=unix:%t/pulse/native"));
}
#[test]
fn quadlet_dbus_present() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    // Default dbus = true with no rules → portal preset → proxied socket
    assert!(q.contains("Volume=%t/podbox/myenv-dbus.sock:/run/podbox/dbus.sock:ro"));
    assert!(q.contains("Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/podbox/dbus.sock"));
}
#[test]
fn quadlet_gpu_device_when_enabled() {
    let config = load_config("full.toml");
    let mut config = config.clone();
    config.integration.gpu = GpuMode::Enabled;
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("AddDevice=/dev/dri"));
}
#[test]
fn quadlet_gpu_nvidia() {
    let config = load_config("full.toml");
    let mut config = config.clone();
    config.integration.gpu = GpuMode::Nvidia;
    let mut env = default_env();
    env.gpu_has_nvidia_uvm = true;
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(q.contains("AddDevice=/dev/dri"));
    assert!(q.contains("AddDevice=-/dev/nvidiactl"));
    assert!(q.contains("AddDevice=-/dev/nvidia0"));
    assert!(q.contains("AddDevice=-/dev/nvidia-uvm"));
    // CDI-absent fallback covers every GPU index plus the modeset and
    // UVM-tools nodes; `-` marks each optional so absent ones are skipped.
    assert!(q.contains("AddDevice=-/dev/nvidia1"));
    assert!(q.contains("AddDevice=-/dev/nvidia7"));
    assert!(q.contains("AddDevice=-/dev/nvidia-modeset"));
    assert!(q.contains("AddDevice=-/dev/nvidia-uvm-tools"));
}
#[test]
fn quadlet_gpu_absent_when_disabled() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(!q.contains("AddDevice="));
}
#[test]
fn quadlet_gpu_auto_detects_dri() {
    let config = load_config("full.toml");
    let mut config = config.clone();
    config.integration.gpu = GpuMode::Auto;
    let mut env = default_env();
    env.gpu_has_dri = true;
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(q.contains("AddDevice=/dev/dri"));
    assert!(!q.contains("nvidia"));
}
#[test]
fn quadlet_gpu_auto_detects_nvidia() {
    let config = load_config("full.toml");
    let mut config = config.clone();
    config.integration.gpu = GpuMode::Auto;
    let mut env = default_env();
    env.gpu_has_nvidia = true;
    env.gpu_has_nvidia_uvm = true;
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(q.contains("AddDevice=-/dev/nvidiactl"));
    assert!(q.contains("AddDevice=-/dev/nvidia0"));
    assert!(q.contains("AddDevice=-/dev/nvidia-modeset"));
    assert!(q.contains("AddDevice=-/dev/nvidia-uvm"));
    assert!(q.contains("AddDevice=-/dev/nvidia-uvm-tools"));
    // Should NOT have /dev/dri (no dri detected)
    assert!(!q.contains("AddDevice=/dev/dri"));
}
#[test]
fn quadlet_gpu_auto_nothing_when_no_gpu() {
    let config = load_config("full.toml");
    let mut config = config.clone();
    config.integration.gpu = GpuMode::Auto;
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(!q.contains("AddDevice="));
}
#[test]
fn quadlet_pipewire_runtime_dir() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Environment=PIPEWIRE_RUNTIME_DIR=%t"));
}
#[test]
fn quadlet_gpu_nvidia_lists_all_nodes_even_without_uvm_on_host() {
    // The fallback lists every /dev/nvidia* node as optional (`-`), so a
    // host probe miss cannot hide a node: absent ones are skipped at start.
    let config = load_config("full.toml");
    let mut config = config.clone();
    config.integration.gpu = GpuMode::Nvidia;
    let mut env = default_env();
    env.gpu_has_nvidia_uvm = false;
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(q.contains("AddDevice=/dev/dri"));
    assert!(q.contains("AddDevice=-/dev/nvidiactl"));
    assert!(q.contains("AddDevice=-/dev/nvidia0"));
    assert!(q.contains("AddDevice=-/dev/nvidia-uvm"));
}

const SECRET_BASE: &str = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;

fn config_with_secret(secret_toml: &str) -> podbox::config::Config {
    podbox::config::Config::parse(&format!("{SECRET_BASE}\n{secret_toml}")).unwrap()
}

#[test]
fn quadlet_systemd_secret_loads_mounts_and_exports() {
    let config = config_with_secret(
        r#"
[[security.secrets]]
name = "gh_token"
source = "systemd"
target = "GH_TOKEN"
path = "/etc/credstore/gh_token"
"#,
    );
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(
        q.contains("LoadCredential=gh_token:/etc/credstore/gh_token"),
        "service must load the credential:\n{q}"
    );
    assert!(
        q.contains("Volume=%d/gh_token:/run/secrets/gh_token:ro,z"),
        "credential directory must be mounted in:\n{q}"
    );
    assert!(
        q.contains("Environment=GH_TOKEN=/run/secrets/gh_token"),
        "env var must point at the container path:\n{q}"
    );
    assert!(
        !q.contains("%d/gh_token\""),
        "no stale host-path env value:\n{q}"
    );
}

#[test]
fn quadlet_systemd_mount_secret_has_no_env_line() {
    let config = config_with_secret(
        r#"
[[security.secrets]]
name = "aws_creds"
type = "mount"
source = "systemd"
path = "/etc/credstore/aws"
"#,
    );
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(
        q.contains("LoadCredential=aws_creds:/etc/credstore/aws"),
        "{q}"
    );
    assert!(
        q.contains("Volume=%d/aws_creds:/run/secrets/aws_creds:ro,z"),
        "{q}"
    );
    assert!(
        !q.lines().any(|l| l.starts_with("Environment=aws_creds=")),
        "mount secrets expose a file, not a variable:\n{q}"
    );
}

#[test]
fn quadlet_systemd_secret_name_must_be_a_safe_filename() {
    let err = podbox::config::Config::parse(&format!(
        "{SECRET_BASE}\n[[security.secrets]]\nname = \"../evil\"\nsource = \"systemd\"\npath = \"/etc/credstore/x\"\n"
    ))
    .expect_err("path traversal in secret name must fail");
    assert!(
        err.to_string().contains("security.secrets[0].name"),
        "{err}"
    );
}

#[test]
fn quadlet_systemd_secret_requires_a_host_path() {
    let err = podbox::config::Config::parse(&format!(
        "{SECRET_BASE}\n[[security.secrets]]\nname = \"gh_token\"\nsource = \"systemd\"\n"
    ))
    .expect_err("missing credential path must fail");
    assert!(
        err.to_string().contains("security.secrets[0].path"),
        "{err}"
    );
}

#[test]
fn quadlet_encrypted_systemd_secret_uses_encrypted_variant() {
    let config = config_with_secret(
        r#"
[[security.secrets]]
name = "db_key"
source = "systemd"
encrypted = true
path = "/etc/credstore/db_key.encrypted"
"#,
    );
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(
        q.contains("LoadCredentialEncrypted=db_key:/etc/credstore/db_key.encrypted"),
        "{q}"
    );
}
