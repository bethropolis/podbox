//! Integration-group checks for `podbox doctor`: the host↔container
//! bridges (Wayland, D-Bus, clipboard, host-exec, hardware, secrets).
//!
//! Extracted verbatim from `doctor.rs`; see `super` for the check surface.

use std::path::Path;

use podbox::config::Config;
use podbox::env::HostEnv;

use super::Check;
use super::fix::{confirm_fix, fix_wayland_socket_ownership};

fn is_user_in_group(username: &str, group: &str) -> bool {
    // Cheap check via /etc/group; covers most setups without NSS.
    if let Ok(content) = std::fs::read_to_string("/etc/group") {
        for line in content.lines() {
            let mut parts = line.split(':');
            let gname = parts.next().unwrap_or("");
            if gname != group {
                continue;
            }
            // format: name:passwd:GID:user_list
            let _passwd = parts.next();
            let _gid = parts.next();
            let members = parts.next().unwrap_or("");
            if members.split(',').any(|m| m.trim() == username) {
                return true;
            }
            // Primary group membership isn't listed in /etc/group user_list.
            // Fall through to nix getgrouplist for completeness.
            break;
        }
    }
    // Fallback via nix getgrouplist if available
    #[cfg(unix)]
    {
        use nix::unistd::{Group, User};
        if let Ok(Some(user)) = User::from_name(username) {
            if let Ok(groups) = nix::unistd::getgrouplist(
                std::ffi::CString::new(username).unwrap().as_c_str(),
                user.gid,
            ) {
                for gid in groups {
                    if let Ok(Some(grp)) = Group::from_gid(gid) {
                        if grp.name == group {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

pub(crate) fn check_wayland(config: &Config, env: &HostEnv, fix: bool) -> Vec<Check> {
    let mut out = Vec::new();
    if config.integration.wayland {
        if let Some(ref socket) = env.wayland_socket {
            out.push(Check::new("Wayland socket", "pass", "found"));
            match socket.metadata() {
                Ok(meta) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        let owner = meta.uid();
                        if owner == env.uid {
                            out.push(Check::new("Wayland socket owner", "pass", "correct"));
                        } else if fix {
                            match fix_wayland_socket_ownership(socket) {
                                Ok(()) => {
                                    out.push(Check::new(
                                        "Wayland socket owner",
                                        "pass",
                                        "fixed via --fix",
                                    ));
                                }
                                Err(e) => {
                                    out.push(Check::new(
                                        "Wayland socket owner",
                                        "fail",
                                        format!("fix failed: {e}"),
                                    ));
                                }
                            }
                        } else {
                            out.push(Check::new(
                                "Wayland socket owner",
                                "warn",
                                format!("owner {} != host UID {}", owner, env.uid),
                            ));
                        }
                    }
                }
                Err(e) => {
                    out.push(Check::new(
                        "Wayland socket",
                        "warn",
                        format!("could not stat: {e}"),
                    ));
                }
            }
        } else {
            out.push(Check::new(
                "Wayland socket",
                "warn",
                "not found (WAYLAND_DISPLAY may not be set)",
            ));
        }
    }
    out
}

pub(crate) fn check_xdg_user_dir() -> Vec<Check> {
    match which::which("xdg-user-dir") {
        Ok(_) => vec![Check::new("xdg-user-dir", "pass", "found")],
        Err(_) => vec![Check::new("xdg-user-dir", "warn", "not found")],
    }
}

pub(crate) fn check_toolchain() -> Vec<Check> {
    let mut out = Vec::new();
    // ── wl-copy / wl-paste / xdg-dbus-proxy ──
    for &(bin, desc) in &[
        ("wl-copy", "clipboard copy from container"),
        ("wl-paste", "clipboard paste to container"),
        ("xdg-dbus-proxy", "D-Bus proxy"),
    ] {
        match which::which(bin) {
            Ok(_) => out.push(Check::new(bin, "pass", "found")),
            Err(_) => out.push(Check::new(
                bin,
                "warn",
                format!("not found — {desc} will fail"),
            )),
        }
    }
    out
}

pub(crate) fn check_host_exec(config: &Config) -> Vec<Check> {
    let mut out = Vec::new();
    // ── host-exec allowlist paths: existence + executable ──
    if config.integration.host_exec.enabled {
        if let Some(map) = &config.integration.host_exec.allowlist {
            let mut sorted: Vec<_> = map.iter().collect();
            sorted.sort_by_key(|(k, _)| *k);
            for (alias, entry) in sorted {
                let path = entry.path();
                let p = Path::new(path);
                let check_name = format!("host-exec: {alias}");
                match std::fs::metadata(p) {
                    Ok(meta) => {
                        if !meta.is_file() {
                            out.push(Check::new(
                                check_name,
                                "fail",
                                format!("'{alias}' → {path} is not a regular file"),
                            ));
                        } else {
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let mode = meta.permissions().mode();
                                if mode & 0o111 == 0 {
                                    out.push(Check::new(
                                        check_name,
                                        "warn",
                                        format!(
                                            "'{alias}' → {path} is not executable (mode {mode:o})"
                                        ),
                                    ));
                                } else {
                                    let filter = if entry.filter_enabled() {
                                        "filter: ON (sanitized)"
                                    } else {
                                        "filter: OFF (unfiltered)"
                                    };
                                    let shim = if entry.shim_enabled() {
                                        "shim: yes"
                                    } else {
                                        "shim: no"
                                    };
                                    out.push(Check::new(
                                        check_name,
                                        "pass",
                                        format!("'{alias}' → {path} [{shim}, {filter}]"),
                                    ));
                                }
                            }
                            #[cfg(not(unix))]
                            {
                                let filter = if entry.filter_enabled() {
                                    "filter: ON (sanitized)"
                                } else {
                                    "filter: OFF (unfiltered)"
                                };
                                let shim = if entry.shim_enabled() {
                                    "shim: yes"
                                } else {
                                    "shim: no"
                                };
                                out.push(Check::new(
                                    check_name,
                                    "pass",
                                    format!("'{alias}' → {path} [{shim}, {filter}]"),
                                ));
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        out.push(Check::new(
                            check_name,
                            "fail",
                            format!("'{alias}' → {path} not found: {e}"),
                        ));
                    }
                    Err(e) => {
                        out.push(Check::new(
                            check_name,
                            "warn",
                            format!("'{alias}' → {path} could not be checked: {e}"),
                        ));
                    }
                }
            }
        }
    }
    out
}

pub(crate) fn check_hardware(config: &Config, env: &HostEnv) -> Vec<Check> {
    let mut out = Vec::new();
    // ── hardware presets: group / device checks ──
    {
        let hw = &config.integration.hardware;
        if hw.joystick {
            if is_user_in_group(&env.username, "input") {
                out.push(Check::new(
                    "hardware: joystick",
                    "pass",
                    "user in 'input' group",
                ));
            } else {
                out.push(Check::new(
                    "hardware: joystick",
                    "warn",
                    "user not in 'input' group — joystick (/dev/input) will be denied",
                ));
            }
        }
        if hw.webcam {
            if is_user_in_group(&env.username, "video") {
                out.push(Check::new(
                    "hardware: webcam",
                    "pass",
                    "user in 'video' group",
                ));
            } else {
                out.push(Check::new(
                    "hardware: webcam",
                    "warn",
                    "user not in 'video' group — /dev/video* will be denied",
                ));
            }
        }
        if hw.serial {
            if is_user_in_group(&env.username, "dialout") || is_user_in_group(&env.username, "uucp")
            {
                out.push(Check::new(
                    "hardware: serial",
                    "pass",
                    "user in 'dialout'/'uucp'",
                ));
            } else {
                out.push(Check::new(
                    "hardware: serial",
                    "warn",
                    "user not in 'dialout' or 'uucp' — /dev/ttyUSB* will be denied",
                ));
            }
        }
        if hw.kvm {
            let p = Path::new("/dev/kvm");
            if p.exists() {
                // Check read/write via metadata permissions or try open
                match std::fs::File::open(p) {
                    Ok(_) => out.push(Check::new("hardware: kvm", "pass", "/dev/kvm accessible")),
                    Err(e) => out.push(Check::new(
                        "hardware: kvm",
                        "warn",
                        format!("/dev/kvm exists but not accessible: {e}"),
                    )),
                }
            } else {
                out.push(Check::new(
                    "hardware: kvm",
                    "warn",
                    "/dev/kvm not found — hardware virtualization unavailable",
                ));
            }
        }
        if hw.yubikey {
            // yubikey uses pcscd socket + hidraw; no group check, just note
            out.push(Check::new(
                "hardware: yubikey",
                "pass",
                "yubikey preset enabled (pcscd + hidraw)",
            ));
        }
    }
    out
}

pub(crate) fn check_secrets(config: &Config) -> Vec<Check> {
    let mut out = Vec::new();
    // ── secrets: verify podman secrets exist ──
    if !config.security.secrets.is_empty() {
        let output = std::process::Command::new("podman")
            .args(["secret", "ls", "--format", "{{.Name}}"])
            .output();
        let available: std::collections::HashSet<String> = match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            _ => std::collections::HashSet::new(),
        };
        for secret in &config.security.secrets {
            let (name, source) = match secret {
                podbox::config::SecretEntry::Simple(n) => {
                    (n.as_str(), podbox::config::SecretSource::Podman)
                }
                podbox::config::SecretEntry::Detailed { name, source, .. } => {
                    (name.as_str(), *source)
                }
            };
            let label = format!("secret: {name}");
            if source == podbox::config::SecretSource::Systemd {
                out.push(Check::new(&label, "pass", "systemd credential passthrough"));
            } else if available.contains(name) {
                out.push(Check::new(&label, "pass", "found in podman secret store"));
            } else {
                out.push(Check::new(
                    &label,
                    "fail",
                    format!("missing — create with `podman secret create {name} -`"),
                ));
            }
        }
    }
    out
}

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
