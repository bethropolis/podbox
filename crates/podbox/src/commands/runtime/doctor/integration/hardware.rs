//! Hardware preset checks: group membership and device accessibility.

use std::path::Path;

use podbox::config::Config;
use podbox::env::HostEnv;

use super::super::Check;

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
