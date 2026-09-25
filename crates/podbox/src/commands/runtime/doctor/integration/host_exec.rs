//! `host-exec` allowlist path checks: existence and executability.

use std::path::Path;

use podbox::config::Config;

use super::super::Check;

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
