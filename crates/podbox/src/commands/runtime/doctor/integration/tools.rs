//! Host-side tool presence checks (`xdg-user-dir`, clipboard, D-Bus proxy).

use super::super::Check;

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
