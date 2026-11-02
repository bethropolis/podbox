//! Cleanup checks: stale guest sockets, dead export shims, and exported
//! desktop entries that no longer launch.

use std::path::Path;

use podbox::config::Config;

use super::super::Check;
use super::super::fix::confirm_fix;

/// True when a socket file belongs to `name`: `<name>.sock` or
/// `<name>-<integration>.sock`.
fn socket_owned_by(name: &str, path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| {
            let (owner, _) = socket_owner(stem);
            owner == name
        })
}

/// Which container a socket file belongs to, and which integration it
/// serves (`None` for the host guest socket `<name>.sock`).
///
/// An exact config-named stem wins, so a box legitimately called
/// `foo-wayland` is not mistaken for `foo`'s wayland socket.
fn socket_owner(stem: &str) -> (String, Option<&'static str>) {
    socket_owner_with(stem, |s| podbox::config::find_config_path(s).is_some())
}

/// [`socket_owner`] with the config-existence check injected, so the naming
/// rule stays testable without touching the real config directory.
fn socket_owner_with(
    stem: &str,
    config_exists: impl Fn(&str) -> bool,
) -> (String, Option<&'static str>) {
    if config_exists(stem) {
        return (stem.to_string(), None);
    }
    for suffix in ["-wayland", "-dbus"] {
        if let Some(owner) = stem.strip_suffix(suffix) {
            return (owner.to_string(), Some(suffix));
        }
    }
    (stem.to_string(), None)
}

/// Does this definition still serve a socket of `kind`? Pure: the two
/// integration switches the socket kinds map to.
fn integration_enabled(cfg: &Config, kind: &str) -> bool {
    match kind {
        "-wayland" => cfg.integration.wayland,
        _ => cfg.integration.dbus,
    }
}

/// True when a socket serves an integration its owner's config no longer
/// enables, or the owner's config is gone entirely.
///
/// Matching the whole stem against a config name flagged every
/// *integration* socket (`dev-dbus.sock`, `dev-wayland.sock`) as stale,
/// including the live ones of a running box: the stem is `<box>-<kind>`,
/// and no config is ever named that. The owner is the stem minus the kind,
/// and liveness comes from the config's own integration switches.
fn socket_is_stale(owner: &str, kind: Option<&str>) -> bool {
    let Some(path) = podbox::config::find_config_path(owner) else {
        return true; // the box itself is gone
    };
    let Some(kind) = kind else {
        return false; // host guest socket of a box that still exists
    };
    // Lenient load, warnings quiet: this scan must stay silent even when a
    // definition on disk is one the strict validator would reject.
    podbox::config::set_parse_warnings_quiet(true);
    let Ok(cfg) = podbox::config::Config::load_lenient(&path) else {
        return false; // cannot tell; never flag
    };
    socket_kind_stale(&cfg, kind)
}

/// Staleness once the owner's definition is loaded: its integration switch
/// is off.
fn socket_kind_stale(cfg: &Config, kind: &str) -> bool {
    !integration_enabled(cfg, kind)
}

pub(crate) fn check_stale_sockets(config: &Config, fix: bool) -> Vec<Check> {
    let mut out = Vec::new();
    // ── Stale sockets (grouped per owner; per-socket lines would flood the
    // summary and inflate the pass/fail ratio) ──
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let sock_dir = Path::new(&runtime_dir).join("podbox");
        let mut stale: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&sock_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "sock")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                    && !stem.is_empty()
                {
                    let (owner, kind) = socket_owner(stem);
                    if socket_is_stale(&owner, kind) {
                        stale.push(path);
                    }
                }
            }
        }
        if !stale.is_empty() {
            if fix && confirm_fix(&format!("Remove {} stale socket(s)", stale.len())) {
                let mut removed = 0usize;
                let mut errors: Vec<String> = Vec::new();
                for path in &stale {
                    match std::fs::remove_file(path) {
                        Ok(()) => removed += 1,
                        Err(e) => errors.push(format!("could not remove {}: {e}", path.display())),
                    }
                }
                if errors.is_empty() {
                    out.push(Check::new(
                        "stale sockets",
                        "pass",
                        format!("removed {removed} via --fix"),
                    ));
                } else {
                    out.push(Check::new(
                        "stale sockets",
                        "fail",
                        format!(
                            "removed {removed}, {} failed: {}",
                            errors.len(),
                            errors.join("; ")
                        ),
                    ));
                }
            } else {
                // Split by owner: leftovers from *this* box mean it dropped
                // an integration (wayland off, say), which is actionable;
                // other boxes' sockets are noise while diagnosing this one.
                let box_name = &config.container.name;
                let (mine, others): (Vec<_>, Vec<_>) = stale
                    .into_iter()
                    .partition(|p| socket_owned_by(box_name, p));
                if !mine.is_empty() {
                    let listed: Vec<String> =
                        mine.iter().map(|p| p.display().to_string()).collect();
                    out.push(Check::new(
                        "stale sockets",
                        "warn",
                        format!(
                            "{} from a setting '{box_name}' no longer uses \
                             (run `podbox doctor --fix` to remove): {}",
                            mine.len(),
                            listed.join("; ")
                        ),
                    ));
                }
                if !others.is_empty() {
                    let listed: Vec<String> =
                        others.iter().map(|p| p.display().to_string()).collect();
                    out.push(Check::new(
                        "stale sockets (other boxes)",
                        "warn",
                        format!(
                            "{} with no config (run `podbox doctor --fix` to remove): {}",
                            others.len(),
                            listed.join("; ")
                        ),
                    ));
                }
            }
        }
    }
    out
}

pub(crate) fn check_dead_exports(fix: bool) -> Vec<Check> {
    let mut out = Vec::new();
    // ── Dead export shims (desktop files + bin shims, one grouped check) ──
    let mut dead: Vec<(std::path::PathBuf, String)> = Vec::new();
    if let Some(apps_dir) = dirs::data_dir().map(|d| d.join("applications")) {
        if let Ok(entries) = std::fs::read_dir(&apps_dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name();
                let fname_str = fname.to_string_lossy();
                if (fname_str.starts_with("podbox-") || fname_str.starts_with("podmgr-"))
                    && fname_str.ends_with(".desktop")
                {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if let Some(exec_line) = content.lines().find(|l| l.starts_with("Exec=")) {
                            let rest = exec_line.strip_prefix("Exec=").unwrap_or("");
                            let args = shell_words::split(rest).unwrap_or_default();
                            let pos = args.iter().position(|a| a == "-C" || a == "--container");
                            if let Some(name) = pos.and_then(|p| args.get(p + 1))
                                && podbox::config::find_config_path(name).is_none()
                            {
                                dead.push((entry.path(), name.clone()));
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(bin_dir) = dirs::home_dir().map(|h| h.join(".local/bin")) {
        if let Ok(entries) = std::fs::read_dir(&bin_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.contains("--container") || content.contains("-C ") {
                        let args = shell_words::split(&content).unwrap_or_default();
                        let pos = args.iter().position(|a| a == "-C" || a == "--container");
                        if let Some(name) = pos.and_then(|p| args.get(p + 1))
                            && podbox::config::find_config_path(name).is_none()
                        {
                            dead.push((path, name.clone()));
                        }
                    }
                }
            }
        }
    }

    if !dead.is_empty() {
        let listed: Vec<String> = dead
            .iter()
            .map(|(p, name)| format!("{} (container '{name}' missing)", p.display()))
            .collect();
        if fix && confirm_fix(&format!("Remove {} dead export(s)", dead.len())) {
            let mut removed = 0usize;
            let mut errors: Vec<String> = Vec::new();
            for (path, _) in &dead {
                match std::fs::remove_file(path) {
                    Ok(()) => removed += 1,
                    Err(e) => errors.push(format!("could not remove {}: {e}", path.display())),
                }
            }
            if errors.is_empty() {
                out.push(Check::new(
                    "dead exports",
                    "pass",
                    format!("removed {removed} via --fix"),
                ));
            } else {
                out.push(Check::new(
                    "dead exports",
                    "fail",
                    format!(
                        "removed {removed}, {} failed: {}",
                        errors.len(),
                        errors.join("; ")
                    ),
                ));
            }
        } else {
            out.push(Check::new(
                "dead exports",
                "warn",
                format!(
                    "{} pointing at missing containers (run `podbox doctor --fix` to remove): {}",
                    dead.len(),
                    listed.join("; ")
                ),
            ));
        }
    }
    out
}

/// Exported desktop entries (066): entries must still be launchable. Two
/// failure modes — a stale `TryExec=` naming a binary that only exists in
/// the container (some launchers hide the entry entirely), and validation
/// errors surfaced by `desktop-file-validate`.
pub(crate) fn check_desktop_entries() -> Vec<Check> {
    let mut out = Vec::new();
    let Some(apps_dir) = dirs::data_dir().map(|d| d.join("applications")) else {
        return out;
    };
    let Ok(entries) = std::fs::read_dir(&apps_dir) else {
        return out;
    };
    // The validator is optional: without it we can still catch TryExec,
    // but not malformed entries.
    let validator = which::which("desktop-file-validate").ok();
    let mut scanned = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let fname_str = fname.to_string_lossy();
        if !(fname_str.starts_with("podbox-") || fname_str.starts_with("podmgr-"))
            || !fname_str.ends_with(".desktop")
        {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        // desktop-file-validate takes a file path, so it runs per entry.
        let validate = match &validator {
            Some(bin) => podbox::process::run_piped(
                &bin.to_string_lossy(),
                &podbox::process::args(&[entry.path().to_string_lossy().to_string()]),
            )
            .ok()
            .map(|o| {
                format!(
                    "{}{}",
                    String::from_utf8_lossy(&o.stderr),
                    String::from_utf8_lossy(&o.stdout)
                )
            }),
            None => None,
        };
        let issues = desktop_entry_issues(&content, validate.as_deref());
        if issues.is_empty() {
            scanned += 1;
        } else {
            bad.push(format!("{}: {}", fname_str, issues.join("; ")));
        }
    }
    if !bad.is_empty() {
        out.push(Check::new(
            "desktop entries",
            "warn",
            format!(
                "{} exported desktop file(s) with problems — re-run `podbox export` to regenerate: {}",
                bad.len(),
                bad.join("; ")
            ),
        ));
    } else if scanned > 0 {
        out.push(Check::new(
            "desktop entries",
            "pass",
            format!("{scanned} exported desktop file(s) valid"),
        ));
    }
    out
}

/// Per-entry diagnostics. `validate` is desktop-file-validate's combined
/// output when the tool ran (None when it is not installed).
fn desktop_entry_issues(content: &str, validate: Option<&str>) -> Vec<String> {
    let mut issues = Vec::new();
    if content
        .lines()
        .any(|l| l.trim_start().starts_with("TryExec="))
    {
        issues.push(
            "TryExec names a binary inside the container, which launchers test against the host — they hide the entry when it is missing"
                .into(),
        );
    }
    if let Some(msg) = validate {
        let msg = msg.trim();
        if !msg.is_empty() {
            issues.push(format!("desktop-file-validate: {msg}"));
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use podbox::config::Config;

    fn base() -> Config {
        Config::parse(
            r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#,
        )
        .expect("minimal config parses")
    }

    #[test]
    fn socket_names_split_into_box_and_kind() {
        // No config named after the full stem: `<box>-<kind>` splits.
        assert_eq!(
            socket_owner_with("dev-wayland", |_| false),
            ("dev".to_string(), Some("-wayland"))
        );
        assert_eq!(
            socket_owner_with("arch-dbus", |_| false),
            ("arch".to_string(), Some("-dbus"))
        );
        // The host guest socket: no suffix, the stem is the box.
        assert_eq!(
            socket_owner_with("dev", |_| false),
            ("dev".to_string(), None)
        );
        // A box genuinely named `foo-wayland` keeps its host socket: the
        // exact match wins over the suffix split.
        assert_eq!(
            socket_owner_with("foo-wayland", |s| s == "foo-wayland"),
            ("foo-wayland".to_string(), None)
        );
    }

    /// The regression: a socket for an integration the box still enables is
    /// live, however stale-looking its `<box>-<kind>` file name is.
    #[test]
    fn live_integration_sockets_are_never_stale() {
        let mut cfg = base();
        cfg.integration.wayland = true;
        assert!(integration_enabled(&cfg, "-wayland"));
        assert!(!socket_kind_stale(&cfg, "-wayland"));
        cfg.integration.wayland = false;
        assert!(!integration_enabled(&cfg, "-wayland"));
        assert!(socket_kind_stale(&cfg, "-wayland"));

        let mut cfg = base();
        cfg.integration.dbus = true;
        assert!(!socket_kind_stale(&cfg, "-dbus"));
        cfg.integration.dbus = false;
        assert!(socket_kind_stale(&cfg, "-dbus"));
    }

    #[test]
    fn clean_entry_has_no_issues() {
        let content = "[Desktop Entry]\nName=demo\nExec=demo\nType=Application\n";
        assert!(desktop_entry_issues(content, Some("")).is_empty());
    }

    #[test]
    fn tryexec_fails_the_entry() {
        let content = "[Desktop Entry]\nTryExec=/usr/bin/app\nExec=demo\n";
        let issues = desktop_entry_issues(content, Some(""));
        assert!(issues.iter().any(|i| i.contains("TryExec")), "{issues:?}");
        assert!(
            desktop_entry_issues(content, None)
                .iter()
                .any(|i| i.contains("TryExec"))
        );
    }

    #[test]
    fn validator_output_counts_as_an_issue() {
        let content = "[Desktop Entry]\nExec=demo\n";
        let issues = desktop_entry_issues(content, Some("demo.desktop: error: Invalid key"));
        assert!(
            issues.iter().any(|i| i.contains("desktop-file-validate")),
            "{issues:?}"
        );
    }
}
