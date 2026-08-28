use std::path::PathBuf;

use podbox::config::Config;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

#[test]
fn parses_full_config() {
    let path = fixtures_dir().join("full.toml");
    let content = std::fs::read_to_string(path).unwrap();
    let cfg = Config::parse(&content).unwrap();

    assert_eq!(cfg.image.base, "fedora:41");
    assert_eq!(cfg.image.name, "myenv");
    assert_eq!(cfg.container.name, "myenv");
    assert_eq!(cfg.container.shell, "fish");
}

#[test]
fn home_tilde_is_expanded() {
    let path = fixtures_dir().join("full.toml");
    let content = std::fs::read_to_string(path).unwrap();
    let cfg = Config::parse(&content).unwrap();

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
fn parses_minimal_config() {
    let path = fixtures_dir().join("minimal.toml");
    let content = std::fs::read_to_string(path).unwrap();
    let cfg = Config::parse(&content).unwrap();

    assert_eq!(cfg.image.base, "fedora:41");
    assert_eq!(cfg.container.name, "minimal");
    assert_eq!(cfg.container.shell, "fish");
    assert_eq!(cfg.integration.gpu, podbox::config::GpuMode::Auto);
    assert!(cfg.integration.wayland);
    assert!(cfg.integration.audio);
    assert!(cfg.integration.dbus);
}

#[test]
fn on_stop_defaults_to_keep() {
    let path = fixtures_dir().join("minimal.toml");
    let content = std::fs::read_to_string(path).unwrap();
    let cfg = Config::parse(&content).unwrap();

    use podbox::config::OnStop;
    assert_eq!(cfg.lifecycle.on_stop, OnStop::Keep);
}

#[test]
fn xdg_dirs_default_all_false() {
    let path = fixtures_dir().join("minimal.toml");
    let content = std::fs::read_to_string(path).unwrap();
    let cfg = Config::parse(&content).unwrap();

    assert!(!cfg.integration.xdg_dirs.documents.is_enabled());
    assert!(!cfg.integration.xdg_dirs.downloads.is_enabled());
    assert!(!cfg.integration.xdg_dirs.pictures.is_enabled());
    assert!(!cfg.integration.xdg_dirs.music.is_enabled());
    assert!(!cfg.integration.xdg_dirs.videos.is_enabled());
    assert!(!cfg.integration.xdg_dirs.desktop.is_enabled());
    assert!(!cfg.integration.xdg_dirs.projects.is_enabled());
}

#[test]
fn wayland_default_is_true() {
    let path = fixtures_dir().join("minimal.toml");
    let content = std::fs::read_to_string(path).unwrap();
    let cfg = Config::parse(&content).unwrap();

    assert!(cfg.integration.wayland);
    assert!(cfg.integration.audio);
}

#[test]
fn no_wayland_config() {
    let path = fixtures_dir().join("no_wayland.toml");
    let content = std::fs::read_to_string(path).unwrap();
    let cfg = Config::parse(&content).unwrap();

    assert!(!cfg.integration.wayland);
    assert!(!cfg.integration.audio);
    assert!(!cfg.integration.dbus);
}

#[test]
fn full_config_packages() {
    let path = fixtures_dir().join("full.toml");
    let content = std::fs::read_to_string(path).unwrap();
    let cfg = Config::parse(&content).unwrap();

    assert_eq!(cfg.image.packages.install.len(), 5);
    assert!(cfg.image.packages.install.contains(&"git".into()));
    assert!(cfg.image.packages.install.contains(&"gcc".into()));
}

#[test]
fn full_config_env() {
    let path = fixtures_dir().join("full.toml");
    let content = std::fs::read_to_string(path).unwrap();
    let cfg = Config::parse(&content).unwrap();

    assert_eq!(cfg.container.env.get("EDITOR"), Some(&"nvim".into()));
    assert_eq!(
        cfg.container.env.get("TERM"),
        Some(&"xterm-256color".into())
    );
}

#[test]
fn full_config_export() {
    let path = fixtures_dir().join("full.toml");
    let content = std::fs::read_to_string(path).unwrap();
    let cfg = Config::parse(&content).unwrap();

    assert_eq!(cfg.integration.export.apps, vec!["gedit", "nautilus"]);
    assert_eq!(cfg.integration.export.bins, vec!["rg", "gcc"]);
}

#[test]
fn host_exec_simple_allowlist_parses() {
    let toml = r#"
[image]
base = "fedora:41"
name = "test"

[container]
name = "test"
home = "~/containers/test"

[integration.host_exec]
enabled = true
[integration.host_exec.allowlist]
flatpak = "/usr/bin/flatpak"
"#;
    let cfg = Config::parse(toml).unwrap();
    assert!(cfg.integration.host_exec.enabled);
    let entry = cfg.integration.host_exec.resolve("flatpak").unwrap();
    assert_eq!(entry.path(), "/usr/bin/flatpak");
    assert!(entry.filter_enabled());
    assert!(entry.shim_enabled());
}

#[test]
fn host_exec_detailed_filter_false_parses() {
    let toml = r#"
[image]
base = "fedora:41"
name = "test"

[container]
name = "test"
home = "~/containers/test"

[integration.host_exec]
enabled = true
[integration.host_exec.allowlist]
git = { path = "/usr/bin/git", filter = false, shim = true }
"#;
    let cfg = Config::parse(toml).unwrap();
    let entry = cfg.integration.host_exec.resolve("git").unwrap();
    assert_eq!(entry.path(), "/usr/bin/git");
    assert!(!entry.filter_enabled());
    assert!(entry.shim_enabled());
}

#[test]
fn host_exec_detailed_shim_false_excluded_from_guest_shims() {
    let toml = r#"
[image]
base = "fedora:41"
name = "test"

[container]
name = "test"
home = "~/containers/test"

[integration.host_exec]
enabled = true
[integration.host_exec.allowlist]
systemctl = { path = "/usr/bin/systemctl", filter = true, shim = false }
flatpak = "/usr/bin/flatpak"
"#;
    let cfg = Config::parse(toml).unwrap();
    let shims = cfg.integration.host_exec.guest_shims();
    assert!(shims.contains(&"flatpak".to_string()));
    assert!(!shims.contains(&"systemctl".to_string()));
    let sys = cfg.integration.host_exec.resolve("systemctl").unwrap();
    assert!(sys.filter_enabled());
    assert!(!sys.shim_enabled());
}

#[test]
fn host_exec_mixed_allowlist_guest_shims_and_validation() {
    let toml = r#"
[image]
base = "fedora:41"
name = "test"

[container]
name = "test"
home = "~/containers/test"

[integration.host_exec]
enabled = true
[integration.host_exec.allowlist]
flatpak = "/usr/bin/flatpak"
git = { path = "/usr/bin/git", filter = false }
code = { path = "/usr/bin/code", filter = false, shim = true }
systemctl = { path = "/usr/bin/systemctl", shim = false }
"#;
    let cfg = Config::parse(toml).unwrap();
    cfg.validate().unwrap();
    let mut shims = cfg.integration.host_exec.guest_shims();
    shims.sort();
    assert_eq!(shims, vec!["code", "flatpak", "git"]);
    assert!(!shims.contains(&"systemctl".to_string()));
    assert!(!cfg
        .integration
        .host_exec
        .resolve("git")
        .unwrap()
        .filter_enabled());
    assert!(cfg
        .integration
        .host_exec
        .resolve("flatpak")
        .unwrap()
        .filter_enabled());
}

#[test]
fn host_exec_invalid_alias_rejected() {
    let toml = r#"
[image]
base = "fedora:41"
name = "test"

[container]
name = "test"
home = "~/containers/test"

[integration.host_exec]
enabled = true
[integration.host_exec.allowlist]
"bad/alias" = "/usr/bin/bad"
"#;
    let result = Config::parse(toml);
    assert!(result.is_err(), "expected validation error for bad alias");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("bad/alias") || err.contains("invalid"),
        "unexpected error: {err}"
    );
}

#[test]
fn host_exec_filter_false_allows_shell_metacharacters_conceptually() {
    use podbox::config::HostExecEntry;
    let filtered = HostExecEntry::Simple("/usr/bin/git".into());
    let unfiltered = HostExecEntry::Detailed {
        path: "/usr/bin/git".into(),
        filter: false,
        shim: true,
    };
    assert!(filtered.filter_enabled());
    assert!(!unfiltered.filter_enabled());
    // The actual bypass is in handle_host_exec: when filter_enabled() is false,
    // validate_host_exec_args is skipped. Verify the entry API correctly
    // distinguishes the two modes.
}
