//! Pre-creation of bind-mount targets inside the container home.
//!
//! Podman creates a missing bind target as root (host uid 100000 under
//! keep-id), so podbox must create every target it mounts into `$HOME`
//! itself. These tests pin both the mapping rules and parity with the
//! generated unit: anything codegen mounts under the home must be covered.

mod common;

use common::*;

use std::path::PathBuf;

use podbox::codegen::quadlet;
use podbox::config::{CustomCacheConfig, HostCacheConfig};
use podbox::quadlet_install::targets::{bind_targets_under_home, precreate_bind_targets};

fn with_temp_home(config: &mut podbox::config::Config) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    config.container.home = tmp.path().join("home");
    tmp
}

fn contains(targets: &[PathBuf], home: &std::path::Path, rel: &str) -> bool {
    targets.iter().any(|p| p == &home.join(rel))
}

#[test]
fn shared_cache_builtins_are_covered() {
    let mut config = load_config("minimal.toml");
    config.storage.shared_caches.builtins.cargo = true;
    config.storage.shared_caches.builtins.npm = true;
    let tmp = with_temp_home(&mut config);
    let home = tmp.path().join("home");

    let targets = bind_targets_under_home(&config, &default_env(), &default_xdg());
    assert!(contains(&targets, &home, ".cargo/registry"));
    assert!(contains(&targets, &home, ".cargo/git"));
    assert!(contains(&targets, &home, ".npm"));
}

#[test]
fn host_cache_builtin_and_custom_are_covered() {
    let mut config = load_config("minimal.toml");
    config.storage.host_caches.builtins.mbx = true;
    config.storage.host_caches.custom.push(HostCacheConfig {
        name: "myapp".into(),
        host_path: "/tmp/nowhere/host-data".into(),
        container_path: "~/.local/share/myapp".into(),
    });
    let tmp = with_temp_home(&mut config);
    let home = tmp.path().join("home");

    let targets = bind_targets_under_home(&config, &default_env(), &default_xdg());
    assert!(contains(&targets, &home, ".cache/mbx"));
    assert!(contains(&targets, &home, ".local/share/myapp"));
}

#[test]
fn extra_mount_dir_source_creates_full_dest() {
    let mut config = load_config("minimal.toml");
    // Destination under the test user's home; source does not exist, so the
    // full destination is created (same as what podman would make).
    config
        .container
        .mounts
        .extra
        .push("/tmp/nowhere/gh:/home/testuser/.config/gh:ro".into());
    let tmp = with_temp_home(&mut config);
    let home = tmp.path().join("home");

    let targets = bind_targets_under_home(&config, &default_env(), &default_xdg());
    assert!(contains(&targets, &home, ".config/gh"));
}

#[test]
fn extra_mount_file_source_creates_parent_only() {
    let mut config = load_config("minimal.toml");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let src_file = src_dir.path().join("settings.toml");
    std::fs::write(&src_file, "x = 1").expect("write source");
    // A file must never be created as a directory: only its parent.
    config.container.mounts.extra.push(format!(
        "{}:/home/testuser/.config/app/settings.toml:ro",
        src_file.display()
    ));
    let tmp = with_temp_home(&mut config);
    let home = tmp.path().join("home");

    let targets = bind_targets_under_home(&config, &default_env(), &default_xdg());
    assert!(contains(&targets, &home, ".config/app"));
    assert!(
        !contains(&targets, &home, ".config/app/settings.toml"),
        "file destination must not be created as a directory"
    );
}

#[test]
fn tilde_and_percent_u_dests_are_covered() {
    let mut config = load_config("minimal.toml");
    config.storage.shared_caches.custom.push(CustomCacheConfig {
        name: "tilde".into(),
        container_path: "~/.cache/tilde-app".into(),
    });
    config
        .container
        .mounts
        .extra
        .push("/tmp/nowhere/x:/home/%u/.cache/pct-u:rw".into());
    let tmp = with_temp_home(&mut config);
    let home = tmp.path().join("home");

    let targets = bind_targets_under_home(&config, &default_env(), &default_xdg());
    assert!(contains(&targets, &home, ".cache/tilde-app"));
    assert!(contains(&targets, &home, ".cache/pct-u"));
}

#[test]
fn destinations_outside_home_are_skipped() {
    let mut config = load_config("minimal.toml");
    config.integration.sync_themes = false;
    config.integration.sync_icons = false;
    config.integration.sync_fonts = false;
    let empty_xdg = podbox::xdg::ResolvedXdgDirs {
        documents: None,
        downloads: None,
        pictures: None,
        music: None,
        videos: None,
        desktop: None,
        projects: None,
    };
    config.container.mounts.extra.extend(
        [
            "/etc/localtime:/etc/localtime:ro",
            "/run/user/1000/bus:/run/podbox/bus:ro",
            "/tmp/nowhere/x:/.flatpak-info:ro",
            "/tmp/nowhere/y:/home/otheruser/stuff:rw",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    let _tmp = with_temp_home(&mut config);

    let targets = bind_targets_under_home(&config, &default_env(), &empty_xdg);
    assert!(
        targets.is_empty(),
        "nothing outside the container home may be pre-created: {targets:?}"
    );
}

#[test]
fn precreate_actually_creates_dirs() {
    let mut config = load_config("minimal.toml");
    config.storage.shared_caches.builtins.cargo = true;
    config.storage.host_caches.builtins.mbx = true;
    let tmp = with_temp_home(&mut config);
    let home = tmp.path().join("home");

    precreate_bind_targets(&config, &default_env(), &default_xdg()).expect("precreate");
    assert!(home.join(".cargo/registry").is_dir());
    assert!(home.join(".cargo/git").is_dir());
    assert!(home.join(".cache/mbx").is_dir());
}

#[test]
fn precreate_leaves_existing_content_alone() {
    let mut config = load_config("minimal.toml");
    config.storage.shared_caches.builtins.npm = true;
    let tmp = with_temp_home(&mut config);
    let home = tmp.path().join("home");

    // Simulate a previous run: the dir exists with user content.
    std::fs::create_dir_all(home.join(".npm")).expect("setup");
    std::fs::write(home.join(".npm/_cacache-marker"), "x").expect("setup");

    precreate_bind_targets(&config, &default_env(), &default_xdg()).expect("precreate");
    assert!(home.join(".npm/_cacache-marker").is_file());
}

#[test]
fn generated_unit_home_dests_are_all_covered() {
    // Parity guard: anything codegen mounts under the container home must be
    // pre-created, or podman makes it root-owned. Parses the real unit, so a
    // new home-relative Volume= line without precreate coverage fails here.
    let mut config = load_config("full.toml");
    config.storage.shared_caches.builtins.cargo = true;
    config.storage.shared_caches.builtins.pip = true;
    config.storage.host_caches.builtins.mbx = true;
    config.storage.host_caches.custom.push(HostCacheConfig {
        name: "myapp".into(),
        host_path: "/tmp/nowhere/host-data".into(),
        container_path: "~/.local/share/myapp".into(),
    });
    let src_dir = tempfile::tempdir().expect("tempdir");
    let src_file = src_dir.path().join("app.toml");
    std::fs::write(&src_file, "x = 1").expect("write source");
    config.container.mounts.extra.push(format!(
        "{}:/home/testuser/.config/fileapp/app.toml:ro",
        src_file.display()
    ));
    config
        .container
        .mounts
        .extra
        .push("/tmp/nowhere/dirapp:/home/testuser/.config/dirapp:rw".into());
    let tmp = with_temp_home(&mut config);
    let home = tmp.path().join("home");
    let env = default_env();

    let unit = quadlet::generate_container(&config, &env, &default_xdg());
    let targets = bind_targets_under_home(&config, &env, &default_xdg());

    // Only the file-mounted destination is expected via its parent.
    let file_dests = [".config/fileapp/app.toml"];

    for line in unit.lines() {
        let Some(rest) = line.strip_prefix("Volume=") else {
            continue;
        };
        let mut parts = rest.splitn(3, ':');
        parts.next();
        let Some(dest) = parts.next() else {
            continue;
        };
        for prefix in ["/home/%u/", "/home/testuser/"] {
            if let Some(rel) = dest.strip_prefix(prefix) {
                let covered_self = targets.iter().any(|p| p == &home.join(rel));
                let covered_parent = PathBuf::from(rel)
                    .parent()
                    .is_some_and(|par| targets.iter().any(|p| p == &home.join(par)));
                assert!(
                    covered_self || (file_dests.contains(&rel) && covered_parent),
                    "unit destination '{dest}' has no pre-created target"
                );
            }
        }
    }

    // And the file destination really is parent-only.
    assert!(contains(&targets, &home, ".config/fileapp"));
    assert!(!contains(&targets, &home, ".config/fileapp/app.toml"));
}
