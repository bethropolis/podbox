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
    assert!(q.contains("AddDevice=-/dev/nvidia-uvm"));
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
fn quadlet_gpu_nvidia_without_uvm() {
    let config = load_config("full.toml");
    let mut config = config.clone();
    config.integration.gpu = GpuMode::Nvidia;
    let mut env = default_env();
    env.gpu_has_nvidia_uvm = false;
    let q = quadlet::generate_container(&config, &env, &default_xdg());
    assert!(q.contains("AddDevice=/dev/dri"));
    assert!(q.contains("AddDevice=-/dev/nvidiactl"));
    assert!(q.contains("AddDevice=-/dev/nvidia0"));
    assert!(!q.contains("nvidia-uvm"));
}
