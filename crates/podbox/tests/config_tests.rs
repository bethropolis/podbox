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
    assert_eq!(cfg.container.shell, "bash");
    assert_eq!(cfg.integration.gpu, podbox::config::GpuMode::Auto);
    assert!(!cfg.integration.wayland);
    assert!(!cfg.integration.audio);
    assert!(!cfg.integration.dbus);
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
fn integration_defaults_to_opt_in() {
    let path = fixtures_dir().join("minimal.toml");
    let content = std::fs::read_to_string(path).unwrap();
    let cfg = Config::parse(&content).unwrap();

    assert!(!cfg.integration.git_identity);
    assert!(!cfg.integration.wayland);
    assert!(!cfg.integration.x11);
    assert!(!cfg.integration.audio);
}

#[test]
fn x11_requires_wayland_and_rejects_host_networking() {
    let base = r#"
[image]
base = "fedora:41"
name = "test"

[container]
name = "test"
home = "~/containers/test"

[integration]
x11 = true
"#;
    let errors = Config::parse(base).expect_err("x11 without Wayland must fail");
    assert!(errors.to_string().contains("x11 needs wayland"));

    let host = format!("{base}\nwayland = true\n\n[network]\nmode = \"host\"\n");
    let errors = Config::parse(&host).expect_err("x11 with host networking must fail");
    assert!(
        errors
            .to_string()
            .contains("host X server's abstract socket"),
        "unexpected validation error: {errors}"
    );

    let offline = format!("{base}\nwayland = true\n\n[network]\noffline = true\n");
    let config = Config::parse(&offline).unwrap();
    assert!(config.integration.x11);
    assert!(config.network.offline);
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
    assert!(
        !cfg.integration
            .host_exec
            .resolve("git")
            .unwrap()
            .filter_enabled()
    );
    assert!(
        cfg.integration
            .host_exec
            .resolve("flatpak")
            .unwrap()
            .filter_enabled()
    );
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
        timeout: None,
    };
    assert!(filtered.filter_enabled());
    assert!(!unfiltered.filter_enabled());
    // The actual bypass is in handle_host_exec: when filter_enabled() is false,
    // validate_host_exec_args is skipped. Verify the entry API correctly
    // distinguishes the two modes.
}

/// The `integration.host_exec.cwd` key parses in all three variants,
/// defaults to `home` when omitted, and a wrong value is a parse error
/// naming the key so a typo in the TOML is discoverable.
#[test]
fn host_exec_cwd_parses_and_rejects_unknown_values() {
    use podbox::config::HostExecCwd;
    let base = r#"
[image]
base = "fedora:41"
name = "test"

[container]
name = "test"
home = "~/containers/test"

[integration.host_exec]
enabled = true
"#;
    let allowlist = r#"
[integration.host_exec.allowlist]
git = "/usr/bin/git"
"#;
    let cfg = Config::parse(&(base.to_owned() + allowlist)).unwrap();
    assert_eq!(cfg.integration.host_exec.cwd, HostExecCwd::Home);

    // `cwd` must sit above the allowlist table: appending it after the
    // `[...]` header would make it an allowlist key instead.
    for (raw, expected) in [
        ("map", HostExecCwd::Map),
        ("home", HostExecCwd::Home),
        ("refuse", HostExecCwd::Refuse),
    ] {
        let toml = base.to_owned() + "cwd = \"" + raw + "\"\n" + allowlist;
        let cfg = Config::parse(&toml).unwrap();
        assert_eq!(cfg.integration.host_exec.cwd, expected, "cwd = {raw:?}");
    }

    let result = Config::parse(&(base.to_owned() + "cwd = \"somewhere\"\n" + allowlist));
    assert!(
        result.is_err(),
        "expected a parse error for cwd = \"somewhere\""
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(err.contains("cwd"), "error should name the cwd key: {err}");
}

/// A per-entry `timeout` parses on the detailed form only, and the serde
/// skip rules keep written TOML minimal: an omitted timeout stays out, a
/// configured one round-trips.
#[test]
fn host_exec_timeout_parses_and_skip_defaults() {
    // Simple entries never carry a timeout; detailed ones do.
    let cfg = Config::parse(
        r#"
[image]
base = "fedora:41"
name = "test"

[container]
name = "test"
home = "~/containers/test"

[integration.host_exec]
enabled = true

[integration.host_exec.allowlist]
git = "/usr/bin/git"
compile = { path = "/usr/local/bin/compile", timeout = 300 }
"#,
    )
    .unwrap();
    assert_eq!(
        cfg.integration
            .host_exec
            .resolve("git")
            .and_then(|e| e.timeout_secs()),
        None,
        "a simple entry has no timeout"
    );
    assert_eq!(
        cfg.integration
            .host_exec
            .resolve("compile")
            .and_then(|e| e.timeout_secs()),
        Some(300)
    );

    // Writing the parsed config must keep default-side keys out: no
    // `cwd` (default `home`) and no `timeout` where none was set.
    let written = toml::to_string(&cfg).unwrap();
    assert!(!written.contains("cwd ="), "{written}");
    // The compile entry keeps its timeout, git stays the simple form.
    assert!(written.contains(r#"timeout = 300"#), "{written}");
    assert!(written.contains(r#"git = "/usr/bin/git""#), "{written}");

    // With a non-default cwd policy the key survives the round trip.
    let cfg_map = Config::parse(
        r#"
[image]
base = "fedora:41"
name = "test"

[container]
name = "test"
home = "~/containers/test"

[integration.host_exec]
enabled = true
cwd = "map"

[integration.host_exec.allowlist]
git = "/usr/bin/git"
"#,
    )
    .unwrap();
    let written = toml::to_string(&cfg_map).unwrap();
    assert!(written.contains("cwd = \"map\""), "{written}");

    // A negative timeout is a TOML type error (unsigned integer).
    let result = Config::parse(
        r#"
[image]
base = "fedora:41"
name = "test"

[container]
name = "test"
home = "~/containers/test"

[integration.host_exec]
enabled = true

[integration.host_exec.allowlist]
git = { path = "/usr/bin/git", timeout = -1 }
"#,
    );
    assert!(
        result.is_err(),
        "expected a parse error for negative timeout"
    );
}

/// A configured timeout must be a whole number of seconds ≥ 1: zero is
/// rejected by validation with the entry named.
#[test]
fn host_exec_timeout_zero_is_rejected() {
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
git = { path = "/usr/bin/git", timeout = 0 }
"#;
    let result = Config::parse(toml);
    assert!(
        result.is_err(),
        "expected a validation error for timeout = 0"
    );
    let err = result.unwrap_err().to_string();
    assert!(err.contains("timeout"), "error should name timeout: {err}");
    assert!(err.contains("git"), "error should name the entry: {err}");
}

/// Every shipped fixture and every bundled profile must still parse:
/// schema changes (new enums, tighter validation) must not break them.
#[test]
fn all_fixtures_and_bundled_profiles_still_parse() {
    let fixtures = fixtures_dir();
    let mut checked = 0;
    let mut entries: Vec<_> = std::fs::read_dir(&fixtures)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    assert!(
        !entries.is_empty(),
        "expected fixture TOMLs in {fixtures:?}"
    );
    for entry in entries {
        let content = std::fs::read_to_string(entry.path()).unwrap();
        Config::parse(&content)
            .unwrap_or_else(|e| panic!("fixture {} must parse: {e:#}", entry.path().display()));
        checked += 1;
    }
    for name in ["cachy", "dev", "fedora", "ubuntu"] {
        // Read the bundled TOMLs directly: `profiles::find` prefers user
        // profiles, so it would not test the shipped files on a host that
        // overrides them.
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("profiles")
            .join(format!("{name}.toml"));
        let toml = std::fs::read_to_string(&path).unwrap();
        Config::parse(&toml)
            .unwrap_or_else(|e| panic!("bundled profile '{name}' must parse: {e:#}"));
        checked += 1;
    }
    assert!(
        checked >= 8,
        "expected fixtures + profiles, checked {checked}"
    );
}

/// The USB hardware switch round-trips: omitted when false so written
/// TOML stays minimal, parsed when true.
#[test]
fn hardware_usb_round_trip() {
    let base = "[image]\nbase = \"fedora:41\"\nname = \"env\"\n[container]\nname = \"env\"\nhome = \"~/env\"\n";
    let off = Config::parse(base).unwrap();
    assert!(!off.integration.hardware.usb);
    let written = toml::to_string(&off).unwrap();
    assert!(!written.contains("usb"), "{written}");

    let on = Config::parse(&(base.to_owned() + "[integration.hardware]\nusb = true\n")).unwrap();
    assert!(on.integration.hardware.usb);
}

/// Definitions written by podbox 0.8.x carry `[dbus]` rules without an
/// `integration.dbus` key (the old emitter skipped it while the bus was
/// default-on). The rules carry the intent: the bus stays on, with a notice.
#[test]
fn dbus_rules_without_stated_switch_keep_the_bus_on() {
    let base = "[image]\nbase = \"fedora:41\"\nname = \"env\"\n[container]\nname = \"env\"\nhome = \"~/env\"\n\n[dbus]\npreset = \"portal\"\n";
    let cfg = Config::parse(base).expect("legacy dbus definition must parse");
    assert!(cfg.integration.dbus);
    assert!(
        cfg.parse_notices
            .iter()
            .any(|notice| notice.contains("integration.dbus")),
        "{:?}",
        cfg.parse_notices
    );

    // The written form names the switch explicitly, so it re-reads the same.
    let round_trip = Config::parse(&toml::to_string(&cfg).unwrap()).unwrap();
    assert!(round_trip.integration.dbus);
    assert!(
        round_trip.parse_notices.is_empty(),
        "{:?}",
        round_trip.parse_notices
    );
}

/// A preset explicitly set to `none` is an opt-out, not intent: no revival.
#[test]
fn dbus_preset_none_means_no_bus() {
    let base = "[image]\nbase = \"fedora:41\"\nname = \"env\"\n[container]\nname = \"env\"\nhome = \"~/env\"\n\n[dbus]\npreset = \"none\"\n";
    let cfg = Config::parse(base).unwrap();
    assert!(!cfg.integration.dbus);
    assert!(cfg.parse_notices.is_empty());
}

/// Stating the switch as false next to rules is a genuine contradiction and
/// stays a hard error.
#[test]
fn dbus_explicitly_off_with_rules_is_rejected() {
    let base = "[image]\nbase = \"fedora:41\"\nname = \"env\"\n[container]\nname = \"env\"\nhome = \"~/env\"\n\n[integration]\ndbus = false\n\n[dbus]\npreset = \"portal\"\n";
    let err = Config::parse(base).unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("integration.dbus"), "{text}");
}
