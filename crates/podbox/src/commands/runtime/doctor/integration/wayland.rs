//! Wayland socket and ownership checks.

use podbox::config::Config;
use podbox::env::HostEnv;

use super::super::Check;
use super::super::fix::fix_wayland_socket_ownership;

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
