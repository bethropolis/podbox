//! Host-group checks for `podbox doctor`: machine/system prerequisites.
//!
//! Extracted verbatim from `doctor.rs`; see `super` for the check surface.

use podbox::config::Config;
use podbox::env::HostEnv;

use super::Check;
use super::fix::{confirm_fix, enable_linger};

/// Check whether a container is managed by systemd (Quadlet).
pub(crate) fn is_systemd_managed(name: &str) -> bool {
    podbox::systemd::is_unit_enabled(name)
}

pub(crate) fn check_podman() -> Vec<Check> {
    let mut out = Vec::new();
    match podbox::podman::podman_version() {
        Ok(ver) if ver.at_least(6, 0) => {
            out.push(Check::new(
                "podman",
                "pass",
                format!(
                    "{}.{}.{} (6.x file-based Quadlet install)",
                    ver.major, ver.minor, ver.patch
                ),
            ));
        }
        Ok(ver) if ver.at_least(5, 6) => {
            out.push(Check::new(
                "podman",
                "pass",
                format!("{}.{}.{} (>= 5.6)", ver.major, ver.minor, ver.patch),
            ));
        }
        Ok(ver) if ver.at_least(5, 5) => {
            out.push(Check::new(
                "podman",
                "warn",
                format!("{}.{}.{} (< 5.6)", ver.major, ver.minor, ver.patch),
            ));
        }
        Ok(ver) => {
            out.push(Check::new(
                "podman",
                "fail",
                format!("{}.{}.{} (< 5.5)", ver.major, ver.minor, ver.patch),
            ));
        }
        Err(_) => {
            out.push(Check::new("podman", "fail", "not found in PATH"));
        }
    }
    out
}

pub(crate) fn check_sub_ids(env: &HostEnv) -> Vec<Check> {
    let mut out = Vec::new();
    match std::fs::read_to_string("/etc/subuid") {
        Ok(content) => {
            let username = &env.username;
            if content.lines().any(|l| l.starts_with(username)) {
                out.push(Check::new(
                    "/etc/subuid",
                    "pass",
                    format!("user '{username}' has sub-UID allocations"),
                ));
            } else {
                out.push(Check::new(
                    "/etc/subuid",
                    "fail",
                    format!("user '{username}' missing from /etc/subuid"),
                ));
            }
        }
        Err(_) => {
            out.push(Check::new(
                "/etc/subuid",
                "warn",
                "could not read /etc/subuid",
            ));
        }
    }

    match std::fs::read_to_string("/etc/subgid") {
        Ok(content) => {
            let username = &env.username;
            if content.lines().any(|l| l.starts_with(username)) {
                out.push(Check::new(
                    "/etc/subgid",
                    "pass",
                    format!("user '{username}' has sub-GID allocations"),
                ));
            } else {
                out.push(Check::new(
                    "/etc/subgid",
                    "fail",
                    format!("user '{username}' missing from /etc/subgid"),
                ));
            }
        }
        Err(_) => {
            out.push(Check::new(
                "/etc/subgid",
                "warn",
                "could not read /etc/subgid",
            ));
        }
    }
    out
}

pub(crate) fn check_embedded_guest() -> Vec<Check> {
    match podbox::guest::PODBOX_GUEST {
        Some(bytes) => {
            vec![Check::new(
                "embedded guest binary",
                "pass",
                format!("{} bytes", bytes.len()),
            )]
        }
        None => {
            vec![Check::new(
                "embedded guest binary",
                "warn",
                "no embedded guest — prebuilt-image build; custom `podbox build` unsupported (use a release binary or source build)",
            )]
        }
    }
}

pub(crate) fn check_linger(config: &Config, fix: bool) -> Vec<Check> {
    let mut out = Vec::new();
    if config.lifecycle.autostart {
        if which::which("loginctl").is_ok() {
            let username = std::env::var("USER").unwrap_or_default();
            if !username.is_empty()
                && let Ok(output) = podbox::process::run_piped(
                    "loginctl",
                    &[
                        "show-user".into(),
                        username.clone().into(),
                        "--property=Linger".into(),
                    ],
                )
            {
                let out_str = String::from_utf8_lossy(&output.stdout);
                if out_str.contains("yes") {
                    out.push(Check::new("loginctl linger", "pass", "enabled"));
                } else if fix {
                    if confirm_fix(&format!("Enable linger for '{username}' (autostart is on)")) {
                        match enable_linger(&username) {
                            Ok(()) => {
                                out.push(Check::new(
                                    "loginctl linger",
                                    "pass",
                                    "enabled via --fix",
                                ));
                            }
                            Err(e) => out.push(Check::new(
                                "loginctl linger",
                                "fail",
                                format!("fix failed: {e}"),
                            )),
                        }
                    } else {
                        out.push(Check::new(
                            "loginctl linger",
                            "warn",
                            "not enabled (declined)",
                        ));
                    }
                } else {
                    out.push(Check::new(
                        "loginctl linger",
                        "warn",
                        format!(
                            "not enabled - autostart won't survive reboot; run `loginctl enable-linger {username}` or `podbox doctor --fix`"
                        ),
                    ));
                }
            }
        }
    }
    out
}

pub(crate) fn check_config_layout(fix: bool) -> Vec<Check> {
    let legacy = podbox::config::find_legacy_root_configs();
    if !legacy.is_empty() {
        let names = legacy
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .collect::<Vec<_>>()
            .join(", ");
        if fix && confirm_fix("Migrate legacy configs from root to profiles/ directory?") {
            match crate::commands::migrate::run_migrate(crate::commands::migrate::MigrateOpts {
                dry_run: false,
                force: false,
            }) {
                Ok(()) => vec![Check::new(
                    "config layout",
                    "pass",
                    "migrated legacy configs to profiles/ directory",
                )],
                Err(e) => vec![Check::new(
                    "config layout",
                    "fail",
                    format!("migration failed: {e}"),
                )],
            }
        } else {
            vec![Check::new(
                "config layout",
                "warn",
                format!(
                    "legacy configs found in root ({names}). Run `podbox migrate` (legacy path removed in v0.8)"
                ),
            )]
        }
    } else {
        vec![Check::new(
            "config layout",
            "pass",
            "using canonical ~/.config/podbox/profiles/",
        )]
    }
}
