//! Containerfile codegen (the `podbox build` layer)
//! Split out of the former monolithic `codegen_tests.rs`.

mod common;

use common::*;

use podbox::codegen::containerfile;

#[test]
fn containerfile_from_line() {
    let config = load_config("full.toml");
    let cf = containerfile::generate(&config, "podbox-guest", &default_env()).unwrap();
    assert!(cf.starts_with("FROM fedora:41\n"));
}
#[test]
fn containerfile_copies_guest_binary() {
    let config = load_config("full.toml");
    let cf = containerfile::generate(&config, "podbox-guest", &default_env()).unwrap();
    assert!(cf.contains("COPY podbox-guest /usr/local/bin/podbox-guest"));
}
#[test]
fn containerfile_sets_container_name_env() {
    let config = load_config("full.toml");
    let cf = containerfile::generate(&config, "podbox-guest", &default_env()).unwrap();
    assert!(cf.contains("ENV PODBOX_CONTAINER=myenv"));
}
#[test]
fn containerfile_has_entrypoint() {
    let config = load_config("full.toml");
    let cf = containerfile::generate(&config, "podbox-guest", &default_env()).unwrap();
    assert!(cf.contains(r#"ENTRYPOINT ["/usr/local/bin/podbox-guest", "--entry"]"#));
}
#[test]
fn containerfile_has_packages() {
    let config = load_config("full.toml");
    let cf = containerfile::generate(&config, "podbox-guest", &default_env()).unwrap();
    assert!(cf.contains("dnf install -y"));
    assert!(cf.contains("git"));
    assert!(cf.contains("gcc"));
}
#[test]
fn containerfile_no_packages_when_empty() {
    let config = load_config("minimal.toml");
    let cf = containerfile::generate(&config, "podbox-guest", &default_env()).unwrap();
    assert!(cf.contains("dnf install -y sudo"));
}
#[test]
fn containerfile_has_custom_run_steps() {
    let config = load_config("full.toml");
    let cf = containerfile::generate(&config, "podbox-guest", &default_env()).unwrap();
    assert!(cf.contains("dnf clean all"));
}
#[test]
fn containerfile_removes_packages_after_install_dnf() {
    let mut config = load_config("minimal.toml");
    config.image.packages.remove = vec!["nano".into()];
    let cf = containerfile::generate(&config, "podbox-guest", &default_env()).unwrap();
    let install = cf.find("dnf install -y").expect("install step present");
    let remove = cf.find("RUN dnf remove -y nano && dnf clean all");
    assert!(remove.is_some(), "dnf remove step present:\n{cf}");
    assert!(
        install < remove.unwrap(),
        "remove must follow install:\n{cf}"
    );
}
#[test]
fn containerfile_removes_packages_apt() {
    let mut config = load_config("minimal.toml");
    config.image.base = "ubuntu:24.04".into();
    config.image.packages.remove = vec!["nano".into()];
    let cf = containerfile::generate(&config, "podbox-guest", &default_env()).unwrap();
    assert!(
        cf.contains("RUN apt-get purge -y nano && apt-get autoremove -y"),
        "apt remove step present:\n{cf}"
    );
}
#[test]
fn containerfile_removes_packages_pacman() {
    let mut config = load_config("minimal.toml");
    config.image.base = "archlinux:latest".into();
    config.image.packages.remove = vec!["nano".into()];
    let cf = containerfile::generate(&config, "podbox-guest", &default_env()).unwrap();
    assert!(
        cf.contains("RUN pacman -Rns --noconfirm nano && pacman -Scc --noconfirm"),
        "pacman remove step present:\n{cf}"
    );
}
#[test]
fn containerfile_includes_base_packages() {
    let config = load_config("minimal.toml");
    let cf = containerfile::generate(&config, "podbox-guest", &default_env()).unwrap();
    assert!(cf.contains("sudo"));
    assert!(cf.contains("curl"));
    assert!(cf.contains("tar"));
    assert!(cf.contains("wget"));
    assert!(cf.contains("coreutils"));
    assert!(cf.contains("diffutils"));
    assert!(cf.contains("findutils"));
    assert!(cf.contains("grep"));
    assert!(cf.contains("sed"));
    assert!(cf.contains("gawk"));
}
