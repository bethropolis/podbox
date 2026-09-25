//! Cleanup checks: stale guest sockets and dead export shims.

use std::path::Path;

use super::super::Check;
use super::super::fix::confirm_fix;

pub(crate) fn check_stale_sockets(fix: bool) -> Vec<Check> {
    let mut out = Vec::new();
    // ── Stale sockets (one grouped check; per-socket lines would flood the
    // summary and inflate the pass/fail ratio) ──
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let sock_dir = Path::new(&runtime_dir).join("podbox");
        let mut stale: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&sock_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "sock")
                    && let Some(name) = path.file_stem().and_then(|s| s.to_str())
                    && !name.is_empty()
                    && podbox::config::find_config_path(name).is_none()
                {
                    stale.push(path);
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
                let listed: Vec<String> = stale.iter().map(|p| p.display().to_string()).collect();
                out.push(Check::new(
                    "stale sockets",
                    "warn",
                    format!(
                        "{} with no config (run `podbox doctor --fix` to remove): {}",
                        stale.len(),
                        listed.join("; ")
                    ),
                ));
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
