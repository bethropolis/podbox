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
            let local_tag = format!("localhost/podbox-{name}:latest");
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
    if let Ok(output) = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&[
            "images",
            "--filter",
            "reference=localhost/podbox-*:snapshot-*",
            "--format",
            "{{.Repository}}:{{.Tag}}",
        ]),
    ) {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().filter(|l| !l.is_empty()) {
            if let Some((repo, full_tag)) = line.rsplit_once(':') {
                if let Some(box_name) = repo.strip_prefix("localhost/podbox-") {
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

#[allow(dead_code)]
fn query_guest_info_via_socket(name: &str) -> anyhow::Result<(String, u32)> {
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
