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

pub(crate) fn check_mount_propagation() -> Vec<Check> {
    // Rootless containers need a shared root mount for nested mounts created
    // after start to be visible; on some distros `/` defaults to private.
    match podbox::process::run_piped(
        "findmnt",
        &podbox::process::args(&["-o", "PROPAGATION", "/"]),
    ) {
        Ok(output) => vec![propagation_check(&String::from_utf8_lossy(&output.stdout))],
        Err(_) => Vec::new(), // no findmnt: nothing useful to say
    }
}

/// Classify the root mount's propagation. `findmnt -o PROPAGATION /` prints
/// a `PROPAGATION` header line and then the value.
fn propagation_check(stdout: &str) -> Check {
    let value = stdout
        .lines()
        .skip(1)
        .map(str::trim)
        .find(|l| !l.is_empty());
    match value {
        Some(v) if v.contains("shared") => Check::new("mount propagation", "pass", "shared"),
        Some(v) => Check::new(
            "mount propagation",
            "warn",
            format!(
                "root mount is '{v}', not shared — nested mounts created after start are invisible; fix with `sudo mount --make-rshared /`"
            ),
        ),
        None => Check::new(
            "mount propagation",
            "warn",
            "could not read propagation of /",
        ),
    }
}

pub(crate) fn check_graph_driver() -> Vec<Check> {
    match podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["info", "--format", "{{.Store.GraphDriverName}}"]),
    ) {
        Ok(output) => {
            let driver = String::from_utf8_lossy(&output.stdout).trim().to_string();
            graph_driver_check(&driver).into_iter().collect()
        }
        Err(_) => Vec::new(),
    }
}

/// Slow storage drivers make builds and large workloads crawl; only
/// `fuse-overlayfs` and `vfs` are avoidable pain. An empty name (stub or
/// pre-4.x podman without the `Store` object) is no data, not a failure.
fn graph_driver_check(driver: &str) -> Option<Check> {
    match driver {
        "" => None,
        "fuse-overlayfs" | "vfs" => Some(Check::new(
            "graph driver",
            "warn",
            format!(
                "'{driver}' is slow and can hit overlay errors — native overlay needs kernel >= 5.13 (see docs/troubleshooting.md)"
            ),
        )),
        other => Some(Check::new("graph driver", "pass", other)),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_propagation_passes() {
        let c = propagation_check("PROPAGATION\nshared\n");
        assert_eq!(c.status, "pass", "{c:?}");
    }

    #[test]
    fn private_propagation_warns_with_fix() {
        let c = propagation_check("PROPAGATION\nprivate\n");
        assert_eq!(c.status, "warn", "{c:?}");
        assert!(c.message.contains("make-rshared"), "{c:?}");
    }

    #[test]
    fn empty_propagation_warns() {
        let c = propagation_check("PROPAGATION\n");
        assert_eq!(c.status, "warn", "{c:?}");
    }

    #[test]
    fn native_overlay_passes() {
        let c = graph_driver_check("overlay").unwrap();
        assert_eq!(c.status, "pass", "{c:?}");
    }

    #[test]
    fn fuse_overlayfs_warns() {
        let c = graph_driver_check("fuse-overlayfs").unwrap();
        assert_eq!(c.status, "warn", "{c:?}");
        let vfs = graph_driver_check("vfs").unwrap();
        assert_eq!(vfs.status, "warn", "{vfs:?}");
    }

    #[test]
    fn absent_driver_name_says_nothing() {
        assert!(graph_driver_check("").is_none());
    }
}
