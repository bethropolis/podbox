//! Quadlet `.container` unit: core directives
//! Split out of the former monolithic `codegen_tests.rs`.

mod common;

use common::*;

use podbox::codegen::quadlet;
use podbox::xdg::ResolvedXdgDir;
use std::path::PathBuf;

#[test]
fn quadlet_container_has_userns_custom() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("UserNS=nomap"));
}
#[test]
fn quadlet_disables_podman_default_network_dependency() {
    let config = load_config("minimal.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("[Quadlet]\nDefaultDependencies=false"));
    let quadlet_at = q.find("[Quadlet]").expect("quadlet group");
    let unit_at = q.find("[Unit]").expect("unit group");
    assert!(quadlet_at < unit_at, "[Quadlet] must precede [Unit]");

    let build = quadlet::generate_build(&config, std::path::Path::new("/tmp/Containerfile"));
    assert!(build.starts_with("[Quadlet]\nDefaultDependencies=false\n"));
    let build_group = build.find("[Build]").expect("build group");
    let quadlet_group = build.find("[Quadlet]").expect("quadlet group");
    assert!(
        quadlet_group < build_group,
        "[Quadlet] must precede [Build]"
    );
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
    // ReadOnly without a writable /run keeps the guest daemon from creating
    // /run/podbox/bin (it fails there at startup), and /tmp is needed by
    // ordinary tooling. Both are tmpfs, so the images stay immutable.
    for target in ["/run", "/tmp", "/var/tmp"] {
        assert!(
            q.contains(&format!("Mount=type=tmpfs,destination={target}")),
            "read-only rootfs needs a writable {target}:\n{q}"
        );
    }
}
#[test]
fn quadlet_container_writable_root_has_no_scratch_tmpfs() {
    let config = load_config("minimal.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(!q.contains("ReadOnly=true"));
    assert!(
        !q.contains("type=tmpfs"),
        "scratch tmpfs is read-only-rootfs only:\n{q}"
    );
}
#[test]
fn quadlet_container_has_cpu_limit() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    // Quadlet has no CpuQuota key; the generator rejects the unit outright, so
    // the limit has to be passed through to podman. full.toml has cpus = "4.0".
    assert!(q.contains("PodmanArgs=--cpus=4"));
    assert!(!q.contains("CpuQuota"));
}

#[test]
fn quadlet_container_cpu_limit_keeps_fractions() {
    let mut config = load_config("minimal.toml");
    config.container.cpus = Some("0.5".into());
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    // A period-based quota cannot express a fraction of a CPU; --cpus can.
    assert!(q.contains("PodmanArgs=--cpus=0.5"));
}

#[test]
fn quadlet_emits_offline_scheduling_cache_and_service_policy() {
    let mut config = load_config("minimal.toml");
    config.network.offline = true;
    config.network.ports = vec!["8080:80".into()];
    config.container.cpu_weight = 260;
    config.storage.shared_caches.builtins.cargo = true;
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
    config.storage.host_caches.builtins.mbx = true;
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
    assert!(q.contains("RunInit=true"));
    assert!(!q.contains("PodmanArgs=--init"));
}
#[test]
fn quadlet_container_uses_native_cap_and_workdir_keys() {
    let config = load_config("full.toml");
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("WorkingDir=/home/%u"));
    assert!(q.contains("AddCapability=DAC_READ_SEARCH"));
    assert!(q.contains("AddCapability=PERFMON"));
    assert!(!q.contains("--cap-add="));
    assert!(!q.contains("--workdir="));
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
    let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

/// Both mechanisms must offer the same built-ins, or the two lists drift
/// and a user reasonably assumes a cache is exclusive to one of them.
#[test]
fn both_cache_mechanisms_offer_every_builtin() {
    let all = podbox::config::BuiltinCaches {
        cargo: true,
        npm: true,
        pnpm: true,
        pip: true,
        uv: true,
        yarn: true,
        bun: true,
        composer: true,
        maven: true,
        gradle: true,
        ccache: true,
        go: true,
        rustup: true,
        mbx: true,
    };
    let mut config = load_config("minimal.toml");
    config.storage.shared_caches.builtins = all;
    config.storage.host_caches.builtins = all;
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    // Same destination either way; the source and flags are what differ.
    for path in [
        "/home/%u/.npm",
        "/home/%u/.local/share/pnpm/store",
        "/home/%u/.cache/pip",
        "/home/%u/.cache/uv",
        "/home/%u/.cache/yarn",
        "/home/%u/.yarn/berry/cache",
        "/home/%u/.bun/install/cache",
        "/home/%u/.cache/composer",
        "/home/%u/.m2/repository",
        "/home/%u/.gradle/caches",
        "/home/%u/.cache/ccache",
        "/home/%u/go/pkg/mod",
        "/home/%u/.rustup",
        "/home/%u/.cache/mbx",
    ] {
        let host_side = path.trim_start_matches("/home/%u/");
        assert!(
            q.contains(&format!("Volume=%h/{host_side}:{path}:rw,z")),
            "host bind mount missing for {path}"
        );
    }
    // Cargo is split across two mounts and never the whole ~/.cargo.
    assert!(q.contains("Volume=podbox-cache-cargo-registry:/home/%u/.cargo/registry:U"));
    assert!(q.contains("Volume=podbox-cache-cargo-git:/home/%u/.cargo/git:U"));
    assert!(!q.contains("Volume=podbox-cache-cargo:/home/%u/.cargo:U"));
    assert!(q.contains("Volume=podbox-cache-yarn-classic-cache:/home/%u/.cache/yarn:U"));
    assert!(q.contains("Volume=podbox-cache-yarn-berry-cache:/home/%u/.yarn/berry/cache:U"));
    assert!(q.contains("Volume=%h/.cargo/registry:/home/%u/.cargo/registry:rw,z"));
    assert!(q.contains("Volume=%h/.cargo/git:/home/%u/.cargo/git:rw,z"));

    // Every shared built-in got a volume, keyed by cache name.
    for name in [
        "npm", "pnpm", "pip", "uv", "bun", "composer", "maven", "gradle", "ccache", "go", "rustup",
        "mbx",
    ] {
        assert!(
            q.contains(&format!("Volume=podbox-cache-{name}:")),
            "shared volume missing for {name}"
        );
    }
}

/// `mbx` must be shareable between containers, not only with the host.
#[test]
fn mbx_is_available_as_a_shared_volume() {
    let mut config = load_config("minimal.toml");
    config.storage.shared_caches.builtins.mbx = true;
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Volume=podbox-cache-mbx:/home/%u/.cache/mbx:U"));
}

/// Enabling every built-in in both mechanisms must not emit a duplicate
/// destination.
#[test]
fn every_builtin_can_be_enabled_at_once() {
    let mut config = load_config("minimal.toml");
    config.storage.shared_caches.builtins = podbox::config::BuiltinCaches {
        cargo: true,
        npm: true,
        pnpm: true,
        pip: true,
        uv: true,
        yarn: true,
        bun: true,
        composer: true,
        maven: true,
        gradle: true,
        ccache: true,
        go: true,
        rustup: true,
        mbx: true,
    };
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Volume=podbox-cache-cargo-registry:/home/%u/.cargo/registry:U"));
    assert!(q.contains("Volume=podbox-cache-cargo-git:/home/%u/.cargo/git:U"));
    assert!(!q.contains("Volume=podbox-cache-cargo:/home/%u/.cargo:U"));
}

#[test]
fn xdg_dirs_default_to_read_only_and_respect_read_write() {
    let config = load_config("full.toml");
    // `default_xdg()` resolves every dir read-only.
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(q.contains("Volume=/home/user/Documents:/home/%u/Documents:ro,z"));
    assert!(q.contains("Volume=/home/user/Downloads:/home/%u/Downloads:ro,z"));

    // The detailed table form is the only way to ask for a writable bind; the
    // Studio emits it for `[integration.xdg_dirs]` entries set to read-write.
    let mut writable = default_xdg();
    writable.documents = Some(ResolvedXdgDir {
        path: PathBuf::from("/home/user/Documents"),
        read_write: true,
    });
    let q = quadlet::generate_container(&config, &default_env(), &writable);
    assert!(q.contains("Volume=/home/user/Documents:/home/%u/Documents:z"));
    assert!(!q.contains("Volume=/home/user/Documents:/home/%u/Documents:ro,z"));
    // Unset dirs stay unmounted.
    assert!(!q.contains("/home/user/Pictures"));
}

#[test]
fn host_cache_with_missing_host_dir_is_skipped_with_a_warning() {
    // A probed real host: `~/.cargo/git` absent, `~/.cargo/registry` present.
    let mut env = default_env();
    env.host_cache_dirs = Some(vec!["~/.cargo/registry".into()]);
    if let Ok(home) = std::env::var("HOME")
        && std::path::Path::new(&format!("{home}/.cache/mbx")).is_dir()
    {
        env.host_cache_dirs
            .as_mut()
            .unwrap()
            .push("~/.cache/mbx".into());
    }
    let config = podbox::config::Config::parse(
        r#"
[image]
base = "fedora:44"
name = "env"
[container]
name = "env"
home = "~/containers/env"
[storage.host_caches]
cargo = true
mbx = true
"#,
    )
    .unwrap();
    let (q, warnings) = quadlet::generate_container_with_warnings(&config, &env, &default_xdg());
    assert!(q.contains("Volume=%h/.cargo/registry"), "{q}");
    assert!(
        !q.contains("/.cargo/git"),
        "missing host dir must be skipped: {q}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("cargo") && w.contains("does not exist")),
        "expected a warning: {warnings:?}"
    );
}

#[test]
fn host_cache_is_never_skipped_for_an_unprobed_preview_host() {
    let mut env = default_env();
    env.host_cache_dirs = None;
    let config = podbox::config::Config::parse(
        r#"
[image]
base = "fedora:44"
name = "env"
[container]
name = "env"
home = "~/containers/env"
[storage.host_caches]
cargo = true
"#,
    )
    .unwrap();
    let (q, warnings) = quadlet::generate_container_with_warnings(&config, &env, &default_xdg());
    assert!(q.contains("Volume=%h/.cargo/registry"), "{q}");
    assert!(q.contains("Volume=%h/.cargo/git"), "{q}");
    assert!(
        !warnings.iter().any(|w| w.contains("host_caches")),
        "a preview must never drop caches: {warnings:?}"
    );
}

#[test]
fn quadlet_full_container_snapshot() {
    // First insta snapshot of a whole `.container` file. Possible only now
    // that map iteration is deterministic; run the binary twice (different
    // hash seeds per process) to confirm stability. Home is pinned: the
    // fixture expands `~` from ambient HOME, which varies by machine and is
    // swapped by sandbox tests running in parallel.
    with_fixed_home("/home/testuser", || {
        let config = load_config("full.toml");
        let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
        insta::assert_snapshot!("full_container", q);
    });
}

#[test]
fn quadlet_env_lines_are_sorted_by_key() {
    // Twenty vars inserted in reverse order: HashMap iteration would come
    // out in hash order, BTreeMap in key order.
    let mut config = load_config("minimal.toml");
    for i in (0..20).rev() {
        config
            .container
            .env
            .values
            .insert(format!("ZVAR{i:02}"), format!("v{i}"));
    }
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    let keys: Vec<&str> = q
        .lines()
        .filter_map(|l| l.strip_prefix("Environment=ZVAR"))
        .map(|rest| rest.split('=').next().unwrap_or(""))
        .collect();
    assert_eq!(keys.len(), 20, "all vars must be emitted:\n{q}");
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "env lines must be sorted by key:\n{q}");
}

#[test]
fn quadlet_same_env_in_different_file_order_gives_same_output() {
    let first = podbox::config::Config::parse(
        "[image]\nbase = \"fedora:41\"\nname = \"env\"\n[container]\nname = \"env\"\nhome = \"~/env\"\n[container.env]\nALPHA = \"1\"\nBETA = \"2\"\nGAMMA = \"3\"\n",
    )
    .unwrap();
    let second = podbox::config::Config::parse(
        "[image]\nbase = \"fedora:41\"\nname = \"env\"\n[container]\nname = \"env\"\nhome = \"~/env\"\n[container.env]\nGAMMA = \"3\"\nALPHA = \"1\"\nBETA = \"2\"\n",
    )
    .unwrap();
    let a = quadlet::generate_container(&first, &default_env(), &default_xdg());
    let b = quadlet::generate_container(&second, &default_env(), &default_xdg());
    assert_eq!(a, b, "output must not depend on file order");
}

#[test]
fn quadlet_on_stop_remove_emits_no_autoremove_and_warns() {
    // `AutoRemove` is not a systemd or Quadlet key; Quadlet containers are
    // always ephemeral, so the setting only produces a warning now.
    let (config, warnings) = podbox::config::Config::parse_with_warnings(
        "[image]\nbase = \"fedora:41\"\nname = \"env\"\n[container]\nname = \"env\"\nhome = \"~/env\"\n[lifecycle]\non_stop = \"remove\"\n",
    )
    .unwrap();
    assert!(
        warnings.iter().any(|w| w.contains("lifecycle.on_stop")),
        "deprecation warning expected: {warnings:?}"
    );
    let q = quadlet::generate_container(&config, &default_env(), &default_xdg());
    assert!(!q.contains("AutoRemove"), "no such unit key:\n{q}");
}
