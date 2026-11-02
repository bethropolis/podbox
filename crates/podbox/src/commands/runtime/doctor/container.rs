//! Container-group checks for `podbox doctor`: this definition's
//! lifecycle artifacts (Quadlet files, memory, guest version/protocol).
//!
//! Extracted verbatim from `doctor.rs`; see `super` for the check surface.

use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use podbox::config::Config;
use podbox::env::HostEnv;

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

/// The guest version reported inside a running container: the daemon's
/// `/run/podbox/guest-version` file first, its `PODBOX_GUEST_VERSION`
/// environment variable as a fallback. `None` when neither answers.
fn guest_version_in_container(name: &str) -> Option<String> {
    for probe in [
        vec!["exec", name, "cat", "/run/podbox/guest-version"],
        vec!["exec", name, "printenv", "PODBOX_GUEST_VERSION"],
    ] {
        if let Ok(output) = std::process::Command::new("podman").args(&probe).output()
            && output.status.success()
        {
            let v = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
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

        // 1) If container is running, ask it directly.
        let is_running = podbox::podman::query_state(name)
            .is_ok_and(|s| s == podbox::podman::ContainerState::Running);
        if is_running {
            guest_ver = guest_version_in_container(name);
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

/// Home ownership: entries under the container home the host user does not
/// own.
///
/// A box that once ran under a different userns mapping leaves foreign-owned
/// entries behind (typically `~/.cache`, created by an in-ns root), and
/// every write into them then fails *inside* the box — mise cannot create
/// `~/.cache/mise`, which surfaces as an unexplained provision failure
/// rather than a permission error. The guest sweeps and repairs these at
/// the next start (`podbox-guest` logs one line); this check makes the
/// state visible from the host, with the container stopped.
///
/// Walks with the same rules as the guest sweep: symlinks are skipped
/// entirely, bind mounts under the home (read-only host shares) are never
/// entered, and the home root itself is not judged — it is a volume the
/// host owns.
pub(crate) fn check_home_ownership(config: &Config, env: &HostEnv) -> Vec<Check> {
    let home = &config.container.home;
    let mut foreign: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    let mounts = std::fs::read_to_string("/proc/self/mountinfo")
        .map(|content| {
            content
                .lines()
                .filter_map(|line| line.split_whitespace().nth(4))
                .filter(|p| Path::new(p).starts_with(home.as_os_str()))
                .map(PathBuf::from)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();

    let mut stack = vec![home.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.uid() != env.uid || meta.gid() != env.gid {
                foreign.push(path.display().to_string());
            } else {
                scanned += 1;
            }
            if meta.is_dir() && !mounts.contains(&path) {
                stack.push(path);
            }
        }
    }

    if foreign.is_empty() {
        return vec![Check::new(
            "home ownership",
            "pass",
            format!(
                "{} entries under {} owned by {}",
                scanned,
                home.display(),
                env.username
            ),
        )];
    }
    let listed: Vec<String> = foreign.iter().take(5).cloned().collect();
    vec![Check::new(
        "home ownership",
        "warn",
        format!(
            "{} entry(ies) under {} not owned by {} (e.g. {}) — left by an earlier \
             userns mapping; writes into them fail inside the box. The guest repairs \
             them at the next start, or fix ownership from the host.",
            foreign.len(),
            home.display(),
            env.username,
            listed.join("; ")
        ),
    )]
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

/// Guest daemon liveness, decided from facts instead of a protocol replay.
///
/// The daemon writes `/run/podbox/guest-version` as it starts, so that file
/// *is* the liveness signal. A handshake probe cannot work from the host:
/// the host socket is served by `podbox serve`, which understands only
/// guest→host messages, so it can never answer a host→guest `GetInfo` — the
/// old probe failed on every container and leaked a raw serde error.
fn guest_daemon_check(running: bool, version: Option<&str>, unit: &str) -> Check {
    match (running, version) {
        (true, Some(v)) => Check::new(
            "guest socket",
            "pass",
            format!("guest daemon reachable via {unit} (guest {v})"),
        ),
        (false, _) => Check::new(
            "guest socket",
            "warn",
            format!(
                "{unit} is active but the container is not running — start it with `podbox start`"
            ),
        ),
        (true, None) => Check::new(
            "guest socket",
            "warn",
            format!(
                "{unit} is active but the guest daemon is not running inside the container — \
                 notifications, clipboard and xdg-open silently fail; check `podbox logs`"
            ),
        ),
    }
}

/// Guest socket (001): the `{name}.socket` unit must be active, and the
/// guest daemon inside the container must be running. Without this, the
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
    if let Some(down) = unit_state_check(active, &unit) {
        return vec![down];
    }
    let running = is_running(name);
    // The version file is the daemon's own startup receipt; asking a
    // stopped container for it would only report the wrong failure.
    let version = if running {
        guest_version_in_container(name)
    } else {
        None
    };
    vec![guest_daemon_check(running, version.as_deref(), &unit)]
}

/// `None` = unit active, the daemon check decides; `Some` = unit is down.
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

    /// A home whose entries the host user owns is clean — the common case,
    /// reported as a pass so the check is visible rather than silent.
    #[test]
    fn home_ownership_passes_when_everything_is_the_user_s() {
        let home = tempfile::tempdir().expect("temp home");
        std::fs::create_dir_all(home.path().join(".local/share/fish")).unwrap();
        std::fs::write(home.path().join(".bashrc"), b"# fish").unwrap();
        std::fs::write(home.path().join(".local/share/fish/fish_history"), b"").unwrap();

        let mut cfg = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "x"
[container]
name = "x"
home = "~/x"
"#,
        )
        .unwrap();
        cfg.container.home = home.path().to_path_buf();
        let mut env = HostEnv::mock_preview();
        env.uid = nix::unistd::getuid().as_raw();
        env.gid = nix::unistd::getgid().as_raw();

        let checks = check_home_ownership(&cfg, &env);
        assert_eq!(checks.len(), 1, "{checks:?}");
        assert_eq!(checks[0].status, "pass", "{:?}", checks[0]);
        // .bashrc, .local, .local/share, .local/share/fish, fish_history.
        assert!(
            checks[0].message.contains("5 entries"),
            "{}",
            checks[0].message
        );
    }

    #[test]
    fn guest_daemon_liveness_reads_the_version_file() {
        // The file is the daemon's startup receipt: present means alive.
        let up = guest_daemon_check(true, Some("v0.9.1"), "alpha.socket");
        assert_eq!(up.status, "pass", "{up:?}");
        assert!(up.message.contains("v0.9.1"), "{up:?}");
        // Active socket, container up, no file: the daemon never started.
        let dead = guest_daemon_check(true, None, "alpha.socket");
        assert_eq!(dead.status, "warn", "{dead:?}");
        assert!(dead.message.contains("podbox logs"), "{dead:?}");
        // Container stopped: say that, not a daemon failure.
        let stopped = guest_daemon_check(false, None, "alpha.socket");
        assert_eq!(stopped.status, "warn", "{stopped:?}");
        assert!(stopped.message.contains("podbox start"), "{stopped:?}");
    }
}
