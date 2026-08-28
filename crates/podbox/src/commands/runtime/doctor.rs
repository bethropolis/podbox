//! Doctor diagnostics for `podbox doctor`.
//!
//! Extracted verbatim from `commands/runtime.rs`; see `super` for the rest
//! of the runtime command surface.

use std::path::Path;

use anyhow::Result;
use owo_colors::{OwoColorize, Stream};
use serde::Serialize;

use podbox::cli::OutputFormat;
use podbox::config::Config;
use podbox::env::HostEnv;

/// Check whether a container is managed by systemd (Quadlet).
pub(crate) fn is_systemd_managed(name: &str) -> bool {
    podbox::systemd::is_unit_enabled(name)
}

#[derive(Serialize)]
struct DoctorEntry {
    group: &'static str,
    name: String,
    status: String,
    message: String,
}

/// Report section for a doctor check. Host = machine/system prerequisites,
/// Container = this definition's lifecycle artifacts, Integration = the
/// host↔container bridges (Wayland, D-Bus, clipboard, exports).
fn group_for(check_name: &str) -> &'static str {
    match check_name {
        "podman" | "/etc/subuid" | "/etc/subgid" | "loginctl linger"
        | "embedded guest binary" => "Host",
        "Quadlet files" | "orphaned snapshot" => "Container",
        _ => "Integration",
    }
}

/// Plain-language summary of what this container can reach on the host.
/// Printed after the checks so exposure can be audited at a glance.
fn print_exposure_summary(config: &Config) {
    let on = |b: bool| if b { "enabled" } else { "off" }.to_string();
    println!("\n{}", "Host exposure".if_supports_color(Stream::Stdout, |s| s.bold()));
    let line = |k: &str, v: String| println!("  {k:<22} {v}");

    line("Home directory", format!(
        "{} (persistent container storage)",
        config.container.home.display()
    ));
    line("Network", if config.network.mode == "host" {
        "host mode - container shares the host's network stack".to_string()
    } else {
        format!("{} ({})", config.network.mode,
            if config.network.ports.is_empty() { "no published ports".to_string() }
            else { format!("published ports: {}", config.network.ports.join(", ")) })
    });
    line("Wayland (GUI)", on(config.integration.wayland));
    line("Audio (PipeWire)", on(config.integration.audio));
    line("GPU", format!("{:?}", config.integration.gpu));
    line("D-Bus", if config.integration.dbus {
        format!("proxied; talk list: {}", {
            let talk = config.dbus_effective_talk();
            if talk.is_empty() { "none".to_string() } else { talk.join(", ") }
        })
    } else {
        "off".to_string()
    });
    line("Clipboard", on(config.integration.clipboard));
    line("Notifications", on(config.integration.notify));
    line("URL opening (xdg-open)", on(config.integration.xdg_open));
    line("SSH agent socket", on(config.integration.ssh_agent));
    line("GPG agent socket", on(config.integration.gpg_agent));
    line("Host exec", match (&config.integration.host_exec.enabled, &config.integration.host_exec.allowlist) {
        (true, Some(list)) => format!("ENABLED - allowlisted commands: {}", list.keys().cloned().collect::<Vec<_>>().join(", ")),
        (true, None) => "ENABLED - no allowlist?".to_string(),
        (false, _) => "off".to_string(),
    });
    line("Extra mounts", if config.container.mounts.extra.is_empty() {
        "none".to_string()
    } else {
        config.container.mounts.extra.join(", ")
    });
}

/// Run diagnostics on the container and host environment.
/// Ask before applying a destructive-ish `--fix` action. Only prompts on a
/// real TTY; scripts must stay non-interactive.
fn confirm_fix(action: &str) -> bool {
    if !podbox::codegen::distros::is_tty() {
        return false;
    }
    dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt(format!("Fix: {action}"))
        .default(false)
        .interact()
        .unwrap_or(false)
}

/// Enable lingering for `user` so autostart units run at boot.
fn enable_linger(user: &str) -> Result<()> {
    let status = std::process::Command::new("loginctl")
        .args(["enable-linger", user])
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run loginctl: {e}"))?;
    if !status.success() {
        anyhow::bail!("loginctl enable-linger failed");
    }
    Ok(())
}

pub fn run_doctor(config: &Config, env: &HostEnv, fix: bool, output: OutputFormat) -> Result<()> {
    let mut entries: Vec<DoctorEntry> = Vec::new();
    let mut passes = 0u32;
    let mut failures = 0u32;

    macro_rules! check {
        ($name:expr, $status:expr, $msg:expr $(,)?) => {{
            entries.push(DoctorEntry {
                group: group_for($name),
                name: $name.to_string(),
                status: $status.to_string(),
                message: $msg.to_string(),
            });
            match $status {
                "pass" => passes += 1,
                "fail" => failures += 1,
                _ => {}
            }
        }};
    }

    match podbox::podman::podman_version() {
        Ok(ver) if ver.at_least(6, 0) => {
            check!(
                "podman",
                "pass",
                format!(
                    "{}.{}.{} (6.x file-based Quadlet install)",
                    ver.major, ver.minor, ver.patch
                )
            );
        }
        Ok(ver) if ver.at_least(5, 6) => {
            check!(
                "podman",
                "pass",
                format!("{}.{}.{} (>= 5.6)", ver.major, ver.minor, ver.patch)
            );
        }
        Ok(ver) if ver.at_least(5, 5) => {
            check!(
                "podman",
                "warn",
                format!("{}.{}.{} (< 5.6)", ver.major, ver.minor, ver.patch)
            );
        }
        Ok(ver) => {
            check!(
                "podman",
                "fail",
                format!("{}.{}.{} (< 5.5)", ver.major, ver.minor, ver.patch)
            );
        }
        Err(_) => {
            check!("podman", "fail", "not found in PATH".to_string());
        }
    }

    if config.integration.wayland {
        if let Some(ref socket) = env.wayland_socket {
            check!("Wayland socket", "pass", "found");
            match socket.metadata() {
                Ok(meta) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        let owner = meta.uid();
                        if owner == env.uid {
                            check!("Wayland socket owner", "pass", "correct");
                        } else if fix {
                            match fix_wayland_socket_ownership(socket) {
                                Ok(()) => {
                                    check!("Wayland socket owner", "pass", "fixed via --fix");
                                }
                                Err(e) => {
                                    check!(
                                        "Wayland socket owner",
                                        "fail",
                                        format!("fix failed: {e}")
                                    );
                                }
                            }
                        } else {
                            check!(
                                "Wayland socket owner",
                                "warn",
                                format!("owner {} != host UID {}", owner, env.uid)
                            );
                        }
                    }
                }
                Err(e) => {
                    check!("Wayland socket", "warn", format!("could not stat: {e}"));
                }
            }
        } else {
            check!(
                "Wayland socket",
                "warn",
                "not found (WAYLAND_DISPLAY may not be set)"
            );
        }
    }

    match which::which("xdg-user-dir") {
        Ok(_) => check!("xdg-user-dir", "pass", "found"),
        Err(_) => check!("xdg-user-dir", "warn", "not found"),
    }

    match std::fs::read_to_string("/etc/subuid") {
        Ok(content) => {
            let username = &env.username;
            if content.lines().any(|l| l.starts_with(username)) {
                check!(
                    "/etc/subuid",
                    "pass",
                    format!("user '{username}' has sub-UID allocations")
                );
            } else {
                check!(
                    "/etc/subuid",
                    "fail",
                    format!("user '{username}' missing from /etc/subuid")
                );
            }
        }
        Err(_) => {
            check!("/etc/subuid", "warn", "could not read /etc/subuid");
        }
    }

    match std::fs::read_to_string("/etc/subgid") {
        Ok(content) => {
            let username = &env.username;
            if content.lines().any(|l| l.starts_with(username)) {
                check!(
                    "/etc/subgid",
                    "pass",
                    format!("user '{username}' has sub-GID allocations")
                );
            } else {
                check!(
                    "/etc/subgid",
                    "fail",
                    format!("user '{username}' missing from /etc/subgid")
                );
            }
        }
        Err(_) => {
            check!("/etc/subgid", "warn", "could not read /etc/subgid");
        }
    }

    match podbox::guest::PODBOX_GUEST {
        Some(bytes) => {
            check!(
                "embedded guest binary",
                "pass",
                format!("{} bytes", bytes.len())
            );
        }
        None => {
            check!(
                "embedded guest binary",
                "warn",
                "no embedded guest — prebuilt-image build; custom `podbox build` unsupported (use a release binary or source build)"
            );
        }
    }

    if config.lifecycle.autostart {
        if which::which("loginctl").is_ok() {
            let username = std::env::var("USER").unwrap_or_default();
            if !username.is_empty()
                && let Ok(output) = podbox::process::run_piped(
                    "loginctl",
                    &[
                        "show-user".into(),
                        username.clone().into(),
                        "--property=Linger".into(),
                    ],
                )
            {
                let out = String::from_utf8_lossy(&output.stdout);
                if out.contains("yes") {
                    check!("loginctl linger", "pass", "enabled");
                } else if fix {
                    if confirm_fix(&format!("Enable linger for '{username}' (autostart is on)")) {
                        match enable_linger(&username) {
                            Ok(()) => check!("loginctl linger", "pass", "enabled via --fix"),
                            Err(e) => check!("loginctl linger", "fail", format!("fix failed: {e}")),
                        }
                    } else {
                        check!("loginctl linger", "warn", "not enabled (declined)");
                    }
                } else {
                    check!(
                        "loginctl linger",
                        "warn",
                        format!("not enabled - autostart won't survive reboot; run `loginctl enable-linger {username}` or `podbox doctor --fix`")
                    );
                }
            }
        }
    }

    if config.lifecycle.quadlet {
        if podbox::quadlet_install::is_installed(&config.container.name) {
            check!("Quadlet files", "pass", "installed");
        } else {
            check!("Quadlet files", "warn", "not found");
        }
    }

    // ── wl-copy / wl-paste / xdg-dbus-proxy ──
    for &(bin, desc) in &[
        ("wl-copy", "clipboard copy from container"),
        ("wl-paste", "clipboard paste to container"),
        ("xdg-dbus-proxy", "D-Bus proxy"),
    ] {
        match which::which(bin) {
            Ok(_) => check!(bin, "pass", "found"),
            Err(_) => check!(bin, "warn", format!("not found — {desc} will fail")),
        }
    }

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
                    && !podbox::config::config_dir()
                        .join(format!("{name}.toml"))
                        .exists()
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
                    check!("stale sockets", "pass", format!("removed {removed} via --fix"));
                } else {
                    check!(
                        "stale sockets",
                        "fail",
                        format!(
                            "removed {removed}, {} failed: {}",
                            errors.len(),
                            errors.join("; ")
                        )
                    );
                }
            } else {
                let listed: Vec<String> =
                    stale.iter().map(|p| p.display().to_string()).collect();
                check!(
                    "stale sockets",
                    "warn",
                    format!(
                        "{} with no config (run `podbox doctor --fix` to remove): {}",
                        stale.len(),
                        listed.join("; ")
                    )
                );
            }
        }
    }

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
                            check!("orphaned snapshot", "warn", format!("{line} (no metadata)"));
                        }
                    }
                }
            }
        }
    }

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
                                && !podbox::config::config_dir()
                                    .join(format!("{name}.toml"))
                                    .exists()
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
                            && !podbox::config::config_dir()
                                .join(format!("{name}.toml"))
                                .exists()
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
                check!("dead exports", "pass", format!("removed {removed} via --fix"));
            } else {
                check!(
                    "dead exports",
                    "fail",
                    format!(
                        "removed {removed}, {} failed: {}",
                        errors.len(),
                        errors.join("; ")
                    )
                );
            }
        } else {
            check!(
                "dead exports",
                "warn",
                format!(
                    "{} pointing at missing containers (run `podbox doctor --fix` to remove): {}",
                    dead.len(),
                    listed.join("; ")
                )
            );
        }
    }

    match output {
        OutputFormat::Json => {
            let report = serde_json::json!({
                "checks": entries,
                "summary": {
                    "passes": passes,
                    "failures": failures,
                    "total": entries.len(),
                }
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Text => {
            // Grouped sections in stable order.
            for group in ["Host", "Container", "Integration"] {
                let section: Vec<_> = entries.iter().filter(|e| e.group == group).collect();
                if section.is_empty() {
                    continue;
                }
                println!(
                    "{}",
                    group.if_supports_color(Stream::Stdout, |s| s.bold())
                );
                for entry in &section {
                    let tag = match entry.status.as_str() {
                        "pass" => "PASS".if_supports_color(Stream::Stdout, |s| s.green()).to_string(),
                        "warn" => "WARN".if_supports_color(Stream::Stdout, |s| s.yellow()).to_string(),
                        "fail" => "FAIL".if_supports_color(Stream::Stdout, |s| s.red()).to_string(),
                        _ => entry.status.clone(),
                    };
                    println!("  [{tag}] {}: {}", entry.name, entry.message);
                }
            }
            println!("\n{passes} / {} checks passed", entries.len());
            print_exposure_summary(config);
        }
    }

    if failures > 0 {
        Err(anyhow::anyhow!("{failures} check(s) failed"))
    } else {
        Ok(())
    }
}

fn fix_wayland_socket_ownership(socket: &Path) -> Result<()> {
    let runtime_dir = socket
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine runtime directory from socket path"))?;

    let output = std::process::Command::new("podman")
        .args(["unshare", "chown", "0:0"])
        .arg(socket)
        .arg(runtime_dir)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run podman unshare: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("podman unshare chown failed: {stderr}");
    }
    Ok(())
}
