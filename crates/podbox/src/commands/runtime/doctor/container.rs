//! Container-group checks for `podbox doctor`: this definition's
//! lifecycle artifacts (Quadlet files, memory, guest version/protocol).
//!
//! Extracted verbatim from `doctor.rs`; see `super` for the check surface.

use podbox::config::Config;

use super::Check;
use super::fix::{confirm_fix, rewrite_memory_raw};

pub(crate) fn check_quadlet(config: &Config) -> Vec<Check> {
    let mut out = Vec::new();
    if config.lifecycle.quadlet {
        if podbox::quadlet_install::is_installed(&config.container.name) {
            out.push(Check::new("Quadlet files", "pass", "installed"));
        } else {
            out.push(Check::new("Quadlet files", "warn", "not found"));
        }
    }
    out
}

pub(crate) fn check_memory(config: &Config, fix: bool) -> Vec<Check> {
    // ── container memory: bare number without unit (e.g. "2" → "2G") ──
    if let Some(ref mem) = config.container.memory {
        let t = mem.trim();
        if podbox::config::validation::is_bare_memory_digits(t) {
            let suggested = format!("{t}G");
            if let Some(path) = podbox::config::find_config_path(&config.container.name) {
                let path_str = path.display().to_string();
                if fix
                    && confirm_fix(&format!(
                        "Rewrite memory '{t}' → '{suggested}' in {path_str}?"
                    ))
                {
                    match rewrite_memory_raw(&path, &suggested) {
                        Ok(()) => vec![Check::new(
                            "memory",
                            "pass",
                            format!("fixed '{t}' → '{suggested}' via --fix"),
                        )],
                        Err(e) => vec![Check::new("memory", "fail", format!("fix failed: {e}"))],
                    }
                } else {
                    vec![Check::new(
                        "memory",
                        "warn",
                        format!(
                            "memory = \"{t}\" has no unit; suggested \"{suggested}\" — run `podbox doctor --fix` to rewrite"
                        ),
                    )]
                }
            } else {
                vec![Check::new(
                    "memory",
                    "warn",
                    format!(
                        "memory = \"{t}\" has no unit; suggested \"{suggested}\" — no config file found to rewrite"
                    ),
                )]
            }
        } else if !podbox::config::validation::is_valid_memory(t) {
            vec![Check::new(
                "memory",
                "fail",
                format!("memory = \"{t}\" invalid (e.g. 2g, 512m)"),
            )]
        } else {
            vec![Check::new("memory", "pass", t.to_string())]
        }
    } else {
        vec![Check::new("memory", "pass", "unlimited")]
    }
}

pub(crate) fn check_guest_version(config: &Config) -> Vec<Check> {
    let mut out = Vec::new();
    // ── guest version / protocol (dual-purpose PODBOX_GUEST_VERSION) ──
    {
        let host_ver = podbox::VERSION.to_string();
        let host_proto = podbox::protocol::PROTOCOL_VERSION.to_string();
        let name = &config.container.name;
        // Try running container file first, then image labels.
        let mut guest_ver: Option<String> = None;
        let mut guest_proto: Option<String> = None;

        // 1) If container is running, try podman exec cat /run/podbox/guest-version
        let is_running = podbox::podman::query_state(name)
            .is_ok_and(|s| s == podbox::podman::ContainerState::Running);
        if is_running {
            if let Ok(output) = std::process::Command::new("podman")
                .args(["exec", name, "cat", "/run/podbox/guest-version"])
                .output()
            {
                if output.status.success() {
                    let v = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !v.is_empty() {
                        guest_ver = Some(v);
                    }
                }
            }
            // Try env as well
            if guest_ver.is_none() {
                if let Ok(output) = std::process::Command::new("podman")
                    .args(["exec", name, "printenv", "PODBOX_GUEST_VERSION"])
                    .output()
                {
                    if output.status.success() {
                        let v = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        if !v.is_empty() {
                            guest_ver = Some(v);
                        }
                    }
                }
            }
        }

        // 2) Fallback to image labels (works even when not running)
        if guest_ver.is_none() || guest_proto.is_none() {
            let local_tag = config.local_image_tag();
            if let Ok(labels) = podbox::labels::fetch(&local_tag) {
                if guest_ver.is_none() {
                    if let Some(v) = labels
                        .get("podbox.guest_version")
                        .or_else(|| labels.get("podmgr.guest_version"))
                    {
                        guest_ver = Some(v.clone());
                    }
                }
                if guest_proto.is_none() {
                    if let Some(v) = labels
                        .get("podbox.protocol_version")
                        .or_else(|| labels.get("podmgr.protocol_version"))
                    {
                        guest_proto = Some(v.clone());
                    }
                }
            }
        }

        match (guest_ver, guest_proto) {
            (Some(gv), Some(gp)) => {
                if gv == host_ver {
                    out.push(Check::new("guest version", "pass", gv));
                } else {
                    out.push(Check::new(
                        "guest version",
                        "warn",
                        format!(
                            "guest {gv} vs host {host_ver} — run `podbox build --rebuild {name}`"
                        ),
                    ));
                }
                if gp == host_proto {
                    out.push(Check::new("protocol", "pass", format!("v{gp}")));
                } else {
                    out.push(Check::new(
                        "protocol",
                        "warn",
                        format!("guest protocol v{gp} vs host v{host_proto} — rebuild"),
                    ));
                }
            }
            (Some(gv), None) => {
                if gv == host_ver {
                    out.push(Check::new("guest version", "pass", gv));
                } else {
                    out.push(Check::new(
                        "guest version",
                        "warn",
                        format!(
                            "guest {gv} vs host {host_ver} — run `podbox build --rebuild {name}`"
                        ),
                    ));
                }
                out.push(Check::new(
                    "protocol",
                    "warn",
                    "unknown guest protocol — rebuild".to_string(),
                ));
            }
            (None, Some(gp)) => {
                out.push(Check::new(
                    "guest version",
                    "warn",
                    "unknown guest version — rebuild".to_string(),
                ));
                if gp == host_proto {
                    out.push(Check::new("protocol", "pass", format!("v{gp}")));
                } else {
                    out.push(Check::new(
                        "protocol",
                        "warn",
                        format!("guest protocol v{gp} vs host v{host_proto} — rebuild"),
                    ));
                }
            }
            (None, None) => {
                if is_running {
                    out.push(Check::new(
                        "guest version",
                        "warn",
                        "guest not reporting version (old image) — rebuild".to_string(),
                    ));
                    out.push(Check::new(
                        "protocol",
                        "warn",
                        "unknown — rebuild".to_string(),
                    ));
                } else {
                    out.push(Check::new(
                        "guest version",
                        "warn",
                        "container not running — cannot query guest version (rebuild to ensure match)".to_string(),
                    ));
                    out.push(Check::new(
                        "protocol",
                        "warn",
                        "container not running".to_string(),
                    ));
                }
            }
        }
    }
    out
}

pub(crate) fn check_orphaned_snapshots() -> Vec<Check> {
    let mut out = Vec::new();
    // ── Orphaned snapshot images ──
    let snapshot_filter = format!(
        "reference={}*:snapshot-*",
        podbox::config::IMAGE_REPO_PREFIX
    );
    if let Ok(output) = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&[
            "images",
            "--filter",
            &snapshot_filter,
            "--format",
            "{{.Repository}}:{{.Tag}}",
        ]),
    ) {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().filter(|l| !l.is_empty()) {
            if let Some((repo, full_tag)) = line.rsplit_once(':') {
                if let Some(box_name) = repo.strip_prefix(podbox::config::IMAGE_REPO_PREFIX) {
                    if let Some(tag) = full_tag.strip_prefix("snapshot-") {
                        let meta_path = podbox::config::config_dir()
                            .join("snapshots")
                            .join(box_name)
                            .join(format!("{tag}.toml"));
                        if !meta_path.exists() {
                            out.push(Check::new(
                                "orphaned snapshot",
                                "warn",
                                format!("{line} (no metadata)"),
                            ));
                        }
                    }
                }
            }
        }
    }
    out
}

/// Handshake with the container's guest daemon as a pseudo-guest and ask
/// for `Info`: the closest thing to "the guest daemon is alive" that works
/// from the host. Also used by the doctor's guest-socket check.
pub(crate) fn query_guest_info_via_socket(name: &str) -> anyhow::Result<(String, u32)> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/run/user/1000"));
    let sock_path = runtime_dir.join("podbox").join(format!("{name}.sock"));
    if !sock_path.exists() {
        anyhow::bail!("socket not found");
    }
    let mut stream = std::os::unix::net::UnixStream::connect(&sock_path)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(2)))?;
    // Handshake as a pseudo-guest
    let hello = podbox::protocol::GuestMessage::Hello {
        protocol_version: podbox::protocol::PROTOCOL_VERSION,
        guest_version: podbox::VERSION.to_string(),
        container: name.to_string(),
        capabilities: vec![],
    };
    podbox::protocol::write_frame(&mut stream, &hello)?;
    let Some(bytes) = podbox::protocol::read_frame(&mut stream)? else {
        anyhow::bail!("no hello ack");
    };
    let _ack: podbox::protocol::HostMessage = serde_json::from_slice(&bytes)?;
    // Now query
    let get = podbox::protocol::HostMessage::GetInfo;
    podbox::protocol::write_frame(&mut stream, &get)?;
    let Some(bytes2) = podbox::protocol::read_frame(&mut stream)? else {
        anyhow::bail!("no info reply");
    };
    let msg: podbox::protocol::GuestMessage = serde_json::from_slice(&bytes2)?;
    match msg {
        podbox::protocol::GuestMessage::Info {
            guest_version,
            protocol_version,
        } => Ok((guest_version, protocol_version)),
        other => anyhow::bail!("unexpected reply: {other:?}"),
    }
}

fn is_running(name: &str) -> bool {
    podbox::podman::query_state(name).is_ok_and(|s| s == podbox::podman::ContainerState::Running)
}

/// The "I have no name!" symptom (012): the shell's uid has no passwd
/// entry in the image, so `$HOME` and the prompt are broken.
pub(crate) fn check_guest_user(config: &Config) -> Vec<Check> {
    let name = &config.container.name;
    if !is_running(name) {
        return Vec::new();
    }
    // Resolve the uid inside the container: with keep-id userns it matches
    // the host uid, but the shell's own `id -u` is what's actually broken.
    match podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["exec", name, "sh", "-c", "getent passwd \"$(id -u)\""]),
    ) {
        Ok(output) => vec![passwd_entry_check(&String::from_utf8_lossy(&output.stdout))],
        Err(_) => Vec::new(),
    }
}

/// `getent` exits non-zero when there is no entry, but an empty stdout
/// from a successful run means the same thing — no passwd entry at all.
fn passwd_entry_check(stdout: &str) -> Check {
    if stdout.trim().is_empty() {
        Check::new(
            "guest user",
            "warn",
            "no passwd entry for the runtime uid — the prompt shows 'I have no name!'; create the user in the image (see docs/troubleshooting.md)",
        )
    } else {
        Check::new(
            "guest user",
            "pass",
            "passwd entry found for the runtime uid",
        )
    }
}

/// Locale handling (068): the image must generate the host `LANG`, or every
/// program prints `setlocale` warnings.
pub(crate) fn check_guest_locale(config: &Config) -> Vec<Check> {
    let lang = std::env::var("LANG").unwrap_or_default().trim().to_string();
    if lang.is_empty() || lang == "C" || lang == "POSIX" {
        return Vec::new();
    }
    let name = &config.container.name;
    if !is_running(name) {
        return Vec::new();
    }
    match podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["exec", name, "locale", "-a"]),
    ) {
        Ok(output) => vec![locale_check(
            &lang,
            &String::from_utf8_lossy(&output.stdout),
        )],
        Err(_) => Vec::new(),
    }
}

fn locale_check(lang: &str, list: &str) -> Check {
    let key = locale_key(lang);
    if list.lines().any(|l| locale_key(l) == key) {
        Check::new("guest locale", "pass", format!("{lang} available"))
    } else {
        Check::new(
            "guest locale",
            "warn",
            format!(
                "host LANG '{lang}' is not generated in the image — programs print setlocale warnings; rebuild with `podbox build --rebuild` to generate it (see docs/troubleshooting.md)"
            ),
        )
    }
}

/// Normalize a locale name for comparison: case-insensitive, `_` and `-`
/// interchangeable, @variant dropped, UTF-8 spelled either way.
fn locale_key(locale: &str) -> String {
    let lower = locale.trim().to_lowercase().replace('_', "-");
    let base = lower.split('@').next().unwrap_or(&lower);
    for suffix in [".utf-8", ".utf8"] {
        if let Some(stem) = base.strip_suffix(suffix) {
            return stem.strip_suffix('.').unwrap_or(stem).to_string();
        }
    }
    base.to_string()
}

/// Guest socket (001): the `{name}.socket` unit must be active, and the
/// guest daemon behind it must answer a test `Hello`. Without this, the
/// interceptors (notify-send, clipboard, xdg-open) silently do nothing.
pub(crate) fn check_guest_socket(config: &Config) -> Vec<Check> {
    let name = &config.container.name;
    let unit = format!("{name}.socket");
    // No user manager, or the box was never installed: the earlier checks
    // cover those, and we cannot distinguish "down" from "uninstalled".
    if !podbox::systemd::is_available() || !podbox::quadlet_install::is_installed(name) {
        return Vec::new();
    }
    // `systemctl is-active` prints active/failed/... and exits accordingly.
    let active = podbox::process::run_piped(
        "systemctl",
        &podbox::process::args(&["--user", "is-active", &unit]),
    )
    .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "active");
    match unit_state_check(active, &unit) {
        Some(c) => vec![c],
        None => vec![hello_probe_check(
            query_guest_info_via_socket(name).map_err(|e| e.to_string()),
        )],
    }
}

/// `None` = unit active, probe can decide; `Some` = unit is down.
fn unit_state_check(active: bool, unit: &str) -> Option<Check> {
    if active {
        None
    } else {
        Some(Check::new(
            "guest socket",
            "warn",
            format!(
                "{unit} is not active — notifications, clipboard and xdg-open silently fail; start the box with `podbox start`"
            ),
        ))
    }
}

fn hello_probe_check(probe: Result<(String, u32), String>) -> Check {
    match probe {
        Ok((gv, proto)) => Check::new(
            "guest socket",
            "pass",
            format!("test hello ok (guest {gv}, protocol v{proto})"),
        ),
        Err(e) => Check::new(
            "guest socket",
            "warn",
            format!("unit active but the test hello failed: {e}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwd_entry_passes_when_getent_prints() {
        let c = passwd_entry_check("bet:x:1000:1000:Bet:/home/bet:/bin/bash\n");
        assert_eq!(c.status, "pass");
    }

    #[test]
    fn missing_passwd_entry_warns() {
        let c = passwd_entry_check("   \n");
        assert_eq!(c.status, "warn");
        assert!(c.message.contains("I have no name!"));
    }

    #[test]
    fn host_locale_listed_in_container_passes() {
        // `locale -a` spells the code set differently than the host does.
        let c = locale_check("de_DE.UTF-8", "C\nC.utf8\nde_DE.utf8\nen_US.utf8\n");
        assert_eq!(c.status, "pass", "{c:?}");
    }

    #[test]
    fn missing_locale_warns_with_rebuild_hint() {
        let c = locale_check("de_DE.UTF-8", "C\nC.utf8\nen_US.utf8\n");
        assert_eq!(c.status, "warn", "{c:?}");
        assert!(c.message.contains("--rebuild"), "{c:?}");
    }

    #[test]
    fn locale_keys_unify_spellings() {
        assert_eq!(locale_key("de_DE.UTF-8"), locale_key("de_DE.utf8"));
        assert_eq!(locale_key("de_DE.UTF-8"), locale_key("de-de"));
        assert_eq!(locale_key("C.UTF-8"), "c");
        // Distinct base locales must not collapse into each other.
        assert_ne!(locale_key("de_DE.utf8"), locale_key("en_US.utf8"));
    }

    #[test]
    fn inactive_socket_warns() {
        let c = unit_state_check(false, "alpha.socket").unwrap();
        assert_eq!(c.status, "warn", "{c:?}");
        assert!(c.message.contains("alpha.socket"), "{c:?}");
    }

    #[test]
    fn active_socket_leaves_probe_to_decide() {
        assert!(unit_state_check(true, "alpha.socket").is_none());
    }

    #[test]
    fn hello_probe_pass_and_fail() {
        let ok = hello_probe_check(Ok(("0.9.0".into(), 1)));
        assert_eq!(ok.status, "pass", "{ok:?}");
        let dead = hello_probe_check(Err("connection refused".into()));
        assert_eq!(dead.status, "warn", "{dead:?}");
        assert!(dead.message.contains("connection refused"), "{dead:?}");
    }
}
