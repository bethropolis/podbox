use std::path::PathBuf;

use podbox::config::Config;
use podbox::labels;

mod common;

use common::with_sandbox_home;

fn load_config(name: &str) -> Config {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let content = std::fs::read_to_string(path).unwrap();
    Config::parse(&content).unwrap()
}

// ---- Image source detection ----

#[test]
fn source_prebuilt_when_image_ref_set() {
    let config = load_config("prebuilt.toml");
    assert!(config.image.source().is_prebuilt());
    match config.image.source() {
        podbox::config::ImageSource::Prebuilt { ref_str } => {
            assert_eq!(ref_str, "ghcr.io/bethropolis/podbox:cachy-latest");
        }
        _ => panic!("expected Prebuilt"),
    }
}

#[test]
fn source_build_when_image_ref_not_set() {
    let config = load_config("full.toml");
    assert!(config.image.source().is_build());
    match config.image.source() {
        podbox::config::ImageSource::Build { base } => {
            assert_eq!(base, "fedora:41");
        }
        _ => panic!("expected Build"),
    }
}

#[test]
fn source_prebuilt_uses_image_ref_directly() {
    // Verify image_ref is used verbatim, not resolved through shorthand
    let config = load_config("prebuilt.toml");
    assert_eq!(
        config.image.image_ref.as_deref(),
        Some("ghcr.io/bethropolis/podbox:cachy-latest")
    );
}

#[test]
fn source_build_has_no_image_ref() {
    let config = load_config("full.toml");
    assert!(config.image.image_ref.is_none());
}

// ---- Containerfile prebuilt generation ----

#[test]
fn containerfile_prebuilt_with_nothing_extra_has_no_overlay() {
    let config = load_config("prebuilt.toml");
    // No packages, no RUN commands: nothing to layer, so no overlay, and
    // the unit points straight at the registry ref.
    assert!(!podbox::codegen::containerfile::prebuilt_overlay_needed(
        &config
    ));
    let overlay = podbox::codegen::containerfile::generate_prebuilt_overlay(
        &config,
        "ghcr.io/bethropolis/podbox:cachy-latest",
        true,
    );
    assert!(overlay.is_none());
    let cf = podbox::codegen::containerfile::generate(&config, "podbox-guest").unwrap();
    assert!(cf.starts_with("FROM ghcr.io/bethropolis/podbox:cachy-latest\n"));
}

#[test]
fn containerfile_prebuilt_overlay_has_packages_run_and_guest_in_order() {
    let mut config = load_config("prebuilt.toml");
    config.image.packages.install = vec!["htop".into(), "ripgrep".into()];
    config.image.run.commands = vec!["echo hello".into()];
    let overlay = podbox::codegen::containerfile::generate_prebuilt_overlay(
        &config,
        "ghcr.io/bethropolis/podbox:cachy-latest",
        true,
    )
    .expect("packages + run commands need an overlay");
    let from = overlay
        .find("FROM ghcr.io/bethropolis/podbox:cachy-latest")
        .unwrap();
    let install = overlay.find("htop ripgrep").unwrap();
    let run = overlay.find("RUN echo hello").unwrap();
    let guest = overlay.find("COPY podbox-guest").unwrap();
    assert!(
        from < install && install < run && run < guest,
        "overlay must order FROM, install, RUN, guest:\n{overlay}"
    );
}

#[test]
fn containerfile_prebuilt_overlay_keeps_cachy_sandbox_relaxation() {
    // The old prebuilt path built its own install string without the
    // CachyOS relaxation; the shared installer must carry it.
    let mut config = load_config("prebuilt.toml");
    config.image.packages.install = vec!["htop".into()];
    let overlay = podbox::codegen::containerfile::generate_prebuilt_overlay(
        &config,
        "ghcr.io/bethropolis/podbox:cachy-latest",
        false,
    )
    .expect("packages need an overlay");
    assert!(overlay.contains("DisableSandboxNetwork"), "{overlay}");
    assert!(!overlay.contains("COPY podbox-guest"), "{overlay}");
}

#[test]
fn containerfile_prebuilt_preview_matches_cli_overlay() {
    // The Studio preview renders through generate_preview; it must equal the
    // CLI overlay built without an embedded guest.
    let mut config = load_config("prebuilt.toml");
    config.image.packages.install = vec!["htop".into()];
    config.image.run.commands = vec!["echo hello".into()];
    // The preview delegates to the same function with the ambient guest
    // flag, so this holds both on the CLI and in the wasm Studio build.
    let preview = podbox::codegen::containerfile::generate_preview(&config, "podbox-guest");
    let overlay = podbox::codegen::containerfile::generate_prebuilt_overlay(
        &config,
        "ghcr.io/bethropolis/podbox:cachy-latest",
        podbox::guest::PODBOX_GUEST.is_some(),
    )
    .unwrap();
    assert_eq!(preview, overlay);
}

#[test]
fn quadlet_prebuilt_with_run_commands_points_at_local_overlay() {
    use common::{default_env, default_xdg};
    use podbox::codegen::quadlet;
    let mut config = load_config("prebuilt.toml");
    config.image.run.commands = vec!["echo hello".into()];
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(
        q.contains("Image=localhost/podbox-prebuilt:latest"),
        "run commands need a local overlay image:\n{q}"
    );
}

#[test]
fn containerfile_custom_has_packages() {
    let config = load_config("full.toml");
    let cf = podbox::codegen::containerfile::generate(&config, "podbox-guest").unwrap();
    assert!(!cf.starts_with("FROM ghcr.io"));
    assert!(cf.contains("dnf install -y"));
    assert!(cf.contains("RUN "));
    // Custom Containerfile should COPY the guest binary
    assert!(cf.contains("COPY podbox-guest /usr/local/bin/podbox-guest"));
}

// ---- Label defaults ----

#[test]
fn label_apply_defaults_prebuilt() {
    let mut config = load_config("prebuilt.toml");
    let labels = std::collections::HashMap::from([
        ("podbox.schema".to_string(), "1".to_string()),
        ("podbox.xdg_dirs.documents".to_string(), "true".to_string()),
        ("podbox.integration.gpu".to_string(), "true".to_string()),
        ("podbox.default_shell".to_string(), "/bin/fish".to_string()),
    ]);
    labels::apply_defaults(&mut config, &labels);
    assert_eq!(config.integration.gpu, podbox::config::GpuMode::Enabled);
    assert!(config.integration.xdg_dirs.documents.is_enabled());
}

#[test]
fn label_apply_defaults_empty_does_not_override() {
    let mut config = load_config("prebuilt.toml");
    let labels = std::collections::HashMap::from([("podbox.schema".to_string(), "1".to_string())]);
    labels::apply_defaults(&mut config, &labels);
    // Should keep existing defaults
    assert_eq!(config.container.shell, "/usr/bin/fish");
    assert_eq!(config.integration.gpu, podbox::config::GpuMode::Auto);
}

#[test]
fn label_apply_defaults_schema_mismatch_returns_early() {
    let mut config = load_config("prebuilt.toml");
    let labels =
        std::collections::HashMap::from([("podbox.protocol_version".to_string(), "2".to_string())]);
    // No podbox.schema label -> apply_defaults returns early
    labels::apply_defaults(&mut config, &labels);
    assert!(config.image.source().is_prebuilt());
}

// ---- Quadlet prebuilt Image ref ----

#[test]
fn quadlet_prebuilt_uses_registry_image() {
    use podbox::codegen::quadlet;
    use podbox::env::HostEnv;
    use podbox::xdg::ResolvedXdgDirs;
    let config = load_config("prebuilt.toml");
    let env = HostEnv {
        uid: 1000,
        username: "testuser".into(),
        home_dir: PathBuf::from("/home/testuser"),
        xdg_runtime_dir: PathBuf::from("/run/user/1000"),
        wayland_display: None,
        wayland_socket: None,
        pipewire_socket: None,
        pulse_dir: None,
        dbus_socket: None,
        gpu_has_dri: false,
        gpu_has_nvidia: false,
        gpu_has_nvidia_uvm: false,
        host_has_localtime: false,
        host_has_timezone_file: false,
        host_has_themes: false,
        host_has_icons: false,
        host_has_fonts: false,
        host_has_local_share_themes: false,
        host_has_local_share_icons: false,
        host_has_local_share_fonts: false,
        host_shell: None,
        host_locale: None,
        gpg_agent_socket: None,
        gpg_home: None,
        ssh_agent_socket: None,
        host_cache_dirs: None,
    };
    let xdg = ResolvedXdgDirs {
        documents: None,
        downloads: None,
        pictures: None,
        music: None,
        videos: None,
        desktop: None,
        projects: None,
    };
    let q = quadlet::generate_container(&config, &env, &xdg);
    assert!(q.contains("Image=ghcr.io/bethropolis/podbox:cachy-latest"));
    assert!(!q.contains("Image=prebuilt.build"));
}

#[test]
fn quadlet_custom_uses_build_ref() {
    use podbox::codegen::quadlet;
    use podbox::env::HostEnv;
    use podbox::xdg::ResolvedXdgDir;
    use podbox::xdg::ResolvedXdgDirs;
    let cfg = load_config("full.toml");
    let env = HostEnv {
        uid: 1000,
        username: "testuser".into(),
        home_dir: PathBuf::from("/home/testuser"),
        xdg_runtime_dir: PathBuf::from("/run/user/1000"),
        wayland_display: Some("wayland-0".into()),
        wayland_socket: Some(PathBuf::from("/run/user/1000/wayland-0")),
        pipewire_socket: Some(PathBuf::from("/run/user/1000/pipewire-0")),
        pulse_dir: Some(PathBuf::from("/run/user/1000/pulse")),
        dbus_socket: Some(PathBuf::from("/run/user/1000/bus")),
        gpu_has_dri: false,
        gpu_has_nvidia: false,
        gpu_has_nvidia_uvm: false,
        host_has_localtime: false,
        host_has_timezone_file: false,
        host_has_themes: false,
        host_has_icons: false,
        host_has_fonts: false,
        host_has_local_share_themes: false,
        host_has_local_share_icons: false,
        host_has_local_share_fonts: false,
        host_shell: None,
        host_locale: None,
        gpg_agent_socket: None,
        gpg_home: None,
        ssh_agent_socket: None,
        host_cache_dirs: None,
    };
    let xdg = ResolvedXdgDirs {
        documents: Some(ResolvedXdgDir {
            path: PathBuf::from("/home/user/Documents"),
            read_write: false,
        }),
        downloads: Some(ResolvedXdgDir {
            path: PathBuf::from("/home/user/Downloads"),
            read_write: false,
        }),
        pictures: None,
        music: None,
        videos: None,
        desktop: None,
        projects: None,
    };
    let q = quadlet::generate_container(&cfg, &env, &xdg);
    assert!(q.contains("Image=localhost/podbox-myenv:latest"));
    assert!(!q.contains("Image=ghcr.io"));
}

#[test]
fn quadlet_has_environment_home() {
    use podbox::codegen::quadlet;
    let config = load_config("full.toml");
    let env = podbox::env::HostEnv {
        uid: 1000,
        username: "testuser".into(),
        home_dir: PathBuf::from("/home/testuser"),
        xdg_runtime_dir: PathBuf::from("/run/user/1000"),
        wayland_display: Some("wayland-0".into()),
        wayland_socket: Some(PathBuf::from("/run/user/1000/wayland-0")),
        pipewire_socket: Some(PathBuf::from("/run/user/1000/pipewire-0")),
        pulse_dir: Some(PathBuf::from("/run/user/1000/pulse")),
        dbus_socket: Some(PathBuf::from("/run/user/1000/bus")),
        gpu_has_dri: false,
        gpu_has_nvidia: false,
        gpu_has_nvidia_uvm: false,
        host_has_localtime: false,
        host_has_timezone_file: false,
        host_has_themes: false,
        host_has_icons: false,
        host_has_fonts: false,
        host_has_local_share_themes: false,
        host_has_local_share_icons: false,
        host_has_local_share_fonts: false,
        host_shell: None,
        host_locale: None,
        gpg_agent_socket: None,
        gpg_home: None,
        ssh_agent_socket: None,
        host_cache_dirs: None,
    };
    let xdg = podbox::xdg::ResolvedXdgDirs {
        documents: None,
        downloads: None,
        pictures: None,
        music: None,
        videos: None,
        desktop: None,
        projects: None,
    };
    let q = quadlet::generate_container(&config, &env, &xdg);
    assert!(q.contains("Environment=HOME=/home/%u"));
}

// ---- Profile loading ----

#[test]
fn profile_cachy_parses() {
    with_sandbox_home(|_| {
        let profile = podbox::profiles::find("cachy").expect("cachy profile exists");
        let cfg = Config::parse(&profile.toml).unwrap();
        assert_eq!(cfg.image.base, "cachy-latest");
        assert!(cfg.image.source().is_prebuilt());
        assert_eq!(cfg.container.shell, "/usr/bin/fish");
    });
}

#[test]
fn profile_fedora_parses() {
    with_sandbox_home(|_| {
        let profile = podbox::profiles::find("fedora").expect("fedora profile exists");
        let cfg = Config::parse(&profile.toml).unwrap();
        assert_eq!(cfg.image.base, "fedora-latest");
        assert!(cfg.image.source().is_prebuilt());
    });
}

#[test]
fn profile_dev_parses() {
    with_sandbox_home(|_| {
        let profile = podbox::profiles::find("dev").expect("dev profile exists");
        let cfg = Config::parse(&profile.toml).unwrap();
        assert_eq!(cfg.image.base, "dev-latest");
        assert!(cfg.integration.wayland);
        assert_eq!(cfg.integration.gpu, podbox::config::GpuMode::Enabled);
    });
}

#[test]
fn profile_ubuntu_parses() {
    with_sandbox_home(|_| {
        let profile = podbox::profiles::find("ubuntu").expect("ubuntu profile exists");
        let cfg = Config::parse(&profile.toml).unwrap();
        assert_eq!(cfg.image.base, "ubuntu-latest");
        assert!(cfg.image.source().is_prebuilt());
        assert_eq!(cfg.container.shell, "/usr/bin/fish");
    });
}

#[test]
fn profile_unknown_returns_none() {
    with_sandbox_home(|_| {
        assert!(podbox::profiles::find("nonexistent").is_none());
    });
}

#[test]
fn profile_list_contains_all() {
    with_sandbox_home(|_| {
        let names = podbox::profiles::list_names();
        assert!(names.iter().any(|n| n == "cachy"));
        assert!(names.iter().any(|n| n == "fedora"));
        assert!(names.iter().any(|n| n == "ubuntu"));
        assert!(names.iter().any(|n| n == "dev"));
    });
}
