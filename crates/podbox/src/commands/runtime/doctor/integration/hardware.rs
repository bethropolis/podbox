//! Hardware preset checks: group membership and device accessibility.

use std::path::Path;

use podbox::config::{Config, GpuMode};
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
        if hw.usb {
            out.extend(check_usb(
                config.security.security_label_disable,
                &env.username,
                Path::new("/dev/bus/usb"),
                Path::new("/sys/fs/selinux/enforce"),
            ));
        }
    }
    // ── NVIDIA CDI spec ──
    // Device nodes alone don't carry the user-space driver libraries, which
    // must match the host kernel driver exactly. A CDI spec (generated by
    // `nvidia-ctk cdi generate`) lets the unit use `nvidia.com/gpu=all`
    // instead; without one, CUDA/Vulkan/GL may fail despite the nodes.
    if matches!(config.integration.gpu, GpuMode::Nvidia)
        || (matches!(config.integration.gpu, GpuMode::Auto) && env.gpu_has_nvidia)
    {
        if env.nvidia_cdi_present {
            out.push(Check::new(
                "hardware: nvidia CDI",
                "pass",
                "NVIDIA CDI spec found — full GPU passthrough available",
            ));
        } else {
            out.push(Check::new(
                "hardware: nvidia CDI",
                "warn",
                "no NVIDIA CDI spec in /etc/cdi or /var/run/cdi — falling back to device nodes whose driver libraries must match the host driver; run `nvidia-ctk cdi generate`",
            ));
        }
    }
    out
}

/// USB passthrough checks for `integration.hardware.usb`.
///
/// `usb_root` stands in for `/dev/bus/usb` and `selinux_enforce` for
/// `/sys/fs/selinux/enforce` so tests can point at temp dirs instead of
/// the host. Accessibility is probed with `access(R_OK|W_OK)` — a real
/// open would claim the device — and zero accessible nodes is a hint,
/// not an error: it is normal when nothing is plugged in.
fn check_usb(
    security_label_disable: bool,
    username: &str,
    usb_root: &Path,
    selinux_enforce: &Path,
) -> Vec<Check> {
    let mut out = Vec::new();
    if !usb_root.exists() {
        out.push(Check::new(
            "hardware: usb",
            "warn",
            "/dev/bus/usb not found — USB passthrough is enabled but the host has no USB bus; the mount is skipped",
        ));
    } else {
        let accessible = count_readable_writable_nodes(usb_root);
        if accessible == 0 {
            out.push(Check::new(
                "hardware: usb",
                "warn",
                format!(
                    "no USB device is accessible to {username} (nothing plugged in, or missing udev rules); install your device's rules (android-tools / android-udev) or add the user to plugdev"
                ),
            ));
        } else {
            out.push(Check::new(
                "hardware: usb",
                "pass",
                format!("{accessible} USB device node(s) accessible"),
            ));
        }
    }
    let enforcing =
        std::fs::read_to_string(selinux_enforce).is_ok_and(|content| content.trim() == "1");
    if enforcing && !security_label_disable {
        out.push(Check::new(
            "hardware: usb",
            "warn",
            "SELinux is enforcing and security_label_disable is false: container_t is denied usb_device_t — set security.security_label_disable = true (the default)",
        ));
    }
    out
}

/// Count device nodes under `usb_root/*/*` the current user can open
/// read-write. Every level-two entry is probed — nodes are character
/// devices, not regular files, so no file-type filtering.
fn count_readable_writable_nodes(usb_root: &Path) -> usize {
    let buses = std::fs::read_dir(usb_root).map(|entries| {
        entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>()
    });
    buses.map_or(0, |dirs| {
        dirs.iter()
            .filter_map(|dir| std::fs::read_dir(dir).ok())
            .flat_map(|entries| entries.filter_map(Result::ok).map(|entry| entry.path()))
            .filter(|node| {
                nix::unistd::access(
                    node,
                    nix::unistd::AccessFlags::R_OK | nix::unistd::AccessFlags::W_OK,
                )
                .is_ok()
            })
            .count()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    fn usb_layout(root: &Path, mode: u32) {
        let bus = root.join("001");
        std::fs::create_dir_all(&bus).unwrap();
        let node = bus.join("002");
        std::fs::write(&node, []).unwrap();
        let mut perms = std::fs::metadata(&node).unwrap().permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(&node, perms).unwrap();
    }

    fn enforce_file(dir: &Path, content: &str) -> std::path::PathBuf {
        let p = dir.join("enforce");
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn missing_bus_warns() {
        let tmp = tempfile::tempdir().unwrap();
        let enforce = enforce_file(tmp.path(), "0");
        let checks = check_usb(true, "user", &tmp.path().join("nope"), &enforce);
        assert_eq!(checks.len(), 1);
        assert!(checks[0].message.contains("no USB bus"), "{checks:?}");
    }

    #[test]
    fn empty_bus_warns_with_hint() {
        // No nodes at all: zero accessible is normal when nothing is
        // plugged in, so the message is a hint naming the user, not an
        // error.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("001")).unwrap();
        let enforce = enforce_file(tmp.path(), "0");
        let checks = check_usb(true, "someuser", tmp.path(), &enforce);
        assert_eq!(checks.len(), 1);
        assert!(checks[0].message.contains("someuser"), "{checks:?}");
    }

    #[test]
    fn accessible_nodes_pass() {
        let tmp = tempfile::tempdir().unwrap();
        usb_layout(tmp.path(), 0o666);
        let enforce = enforce_file(tmp.path(), "0");
        let checks = check_usb(true, "user", tmp.path(), &enforce);
        assert!(
            checks.iter().any(|c| c.message.contains("accessible")),
            "{checks:?}"
        );
    }

    #[test]
    fn selinux_enforcing_without_label_disable_warns() {
        let tmp = tempfile::tempdir().unwrap();
        usb_layout(tmp.path(), 0o666);
        let enforce = enforce_file(tmp.path(), "1");
        let warns: Vec<_> = check_usb(false, "user", tmp.path(), &enforce)
            .into_iter()
            .filter(|c| c.message.contains("SELinux"))
            .collect();
        assert_eq!(warns.len(), 1, "expected the SELinux warning");
        let no_warn = check_usb(true, "user", tmp.path(), &enforce);
        assert!(
            no_warn.iter().all(|c| !c.message.contains("SELinux")),
            "{no_warn:?}"
        );
    }
}
