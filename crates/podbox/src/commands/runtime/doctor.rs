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
        "podman"
        | "/etc/subuid"
        | "/etc/subgid"
        | "loginctl linger"
        | "embedded guest binary"
        | "config layout" => "Host",
        "Quadlet files" | "orphaned snapshot" | "memory" | "guest version" | "protocol" => {
            "Container"
        }
        _ => "Integration",
    }
}

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

/// Plain-language summary of what this container can reach on the host.
/// Printed after the checks so exposure can be audited at a glance.
/// Container-specific rows (home, network, mounts) are at the bottom.
fn print_exposure_summary(config: &Config) {
    let on = |b: bool| if b { "enabled" } else { "off" }.to_string();
    let active = podbox::config::read_active_context();
    let is_active = active.as_deref() == Some(config.container.name.as_str());
    let title = if is_active {
        format!(
            "Host exposure — container '{}' (active)",
            config.container.name
        )
    } else {
        format!("Host exposure — container '{}'", config.container.name)
    };
    println!(
        "\n{}",
        title.if_supports_color(Stream::Stdout, |s| s.bold())
    );
    let line = |k: &str, v: String| println!("  {k:<22} {v}");

    // Integration / host-level first
    line("Wayland (GUI)", on(config.integration.wayland));
    line("Audio (PipeWire)", on(config.integration.audio));
    line("GPU", format!("{:?}", config.integration.gpu));
    line(
        "D-Bus",
        if config.integration.dbus {
            format!("proxied; talk list: {}", {
                let talk = config.dbus_effective_talk();
                if talk.is_empty() {
                    "none".to_string()
                } else {
                    talk.join(", ")
                }
            })
        } else {
            "off".to_string()
        },
    );
    line("Clipboard", on(config.integration.clipboard));
    line("Notifications", on(config.integration.notify));
    line("URL opening (xdg-open)", on(config.integration.xdg_open));
    line("SSH agent socket", on(config.integration.ssh_agent));
    line("GPG agent socket", on(config.integration.gpg_agent));
    match (
        &config.integration.host_exec.enabled,
        &config.integration.host_exec.allowlist,
    ) {
        (true, Some(list)) if !list.is_empty() => {
            line("Host exec", "ENABLED".to_string());
            let mut entries: Vec<_> = list.iter().collect();
            entries.sort_by_key(|(k, _)| *k);
            for (alias, entry) in entries {
                let shim = if entry.shim_enabled() { "yes" } else { "no" };
                let filter = if entry.filter_enabled() {
                    "ON (sanitized)"
                } else {
                    "OFF (unfiltered)"
                };
                // Keep alignment pleasant without over-formatting: bullet line
                println!(
                    "    • {:<12} → {:<28} [shim: {}, filter: {}]",
                    alias,
                    entry.path(),
                    shim,
                    filter
                );
            }
        }
        (true, Some(_)) => {
            line("Host exec", "ENABLED - allowlist empty".to_string());
        }
        (true, None) => {
            line("Host exec", "ENABLED - no allowlist?".to_string());
        }
        (false, _) => {
            line("Host exec", "off".to_string());
        }
    }
    // Hardware presets
    {
        let hw = &config.integration.hardware;
        let enabled: Vec<&str> = [
            hw.joystick.then_some("joystick"),
            hw.webcam.then_some("webcam"),
            hw.yubikey.then_some("yubikey"),
            hw.serial.then_some("serial"),
            hw.kvm.then_some("kvm"),
        ]
        .into_iter()
        .flatten()
        .collect();
        if enabled.is_empty() {
            line("Hardware presets", "none".to_string());
        } else {
            line("Hardware presets", enabled.join(", "));
        }
    }
    // Secrets
    if config.security.secrets.is_empty() {
        line("Secrets", "none".to_string());
    } else {
        line(
            "Secrets",
            format!("{} declared", config.security.secrets.len()),
        );
        for secret in &config.security.secrets {
            match secret {
                podbox::config::SecretEntry::Simple(name) => {
                    println!("    • {name} (env → {name}, podman)");
                }
                podbox::config::SecretEntry::Detailed {
                    name,
                    secret_type,
                    target,
                    mode,
                    source,
                } => {
                    let src = match source {
                        podbox::config::SecretSource::Podman => "podman",
                        podbox::config::SecretSource::Systemd => "systemd",
                    };
                    let tgt = target.as_deref().unwrap_or(name);
                    let extra = match secret_type {
                        podbox::config::SecretType::Mount => {
                            format!(", mode {}", mode.as_deref().unwrap_or("0400"))
                        }
                        _ => String::new(),
                    };
                    println!("    • {name} ({secret_type:?} → {tgt}{extra}, {src})");
                }
            }
        }
    }
    // Container-specific at the bottom
    line(
        "Home directory",
        format!(
            "{} (persistent container storage)",
            config.container.home.display()
        ),
    );
    line(
        "Network",
        if config.network.mode == "host" {
            "host mode - container shares the host's network stack".to_string()
        } else {
            format!(
                "{} ({})",
                config.network.mode,
                if config.network.ports.is_empty() {
                    "no published ports".to_string()
                } else {
                    format!("published ports: {}", config.network.ports.join(", "))
                }
            )
        },
    );
    line(
        "Extra mounts",
        if config.container.mounts.extra.is_empty() {
            "none".to_string()
        } else {
            config.container.mounts.extra.join(", ")
        },
    );
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
                        format!(
                            "not enabled - autostart won't survive reboot; run `loginctl enable-linger {username}` or `podbox doctor --fix`"
                        )
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
                            check!(
                                &check_name,
                                "fail",
                                format!("'{alias}' → {path} is not a regular file")
                            );
                        } else {
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let mode = meta.permissions().mode();
                                if mode & 0o111 == 0 {
                                    check!(
                                        &check_name,
                                        "warn",
                                        format!(
                                            "'{alias}' → {path} is not executable (mode {mode:o})"
                                        )
                                    );
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
                                    check!(
                                        &check_name,
                                        "pass",
                                        format!("'{alias}' → {path} [{shim}, {filter}]")
                                    );
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
                                check!(
                                    &check_name,
                                    "pass",
                                    format!("'{alias}' → {path} [{shim}, {filter}]")
                                );
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        check!(
                            &check_name,
                            "fail",
                            format!("'{alias}' → {path} not found: {e}")
                        );
                    }
                    Err(e) => {
                        check!(
                            &check_name,
                            "warn",
                            format!("'{alias}' → {path} could not be checked: {e}")
                        );
                    }
                }
            }
        }
    }

    // ── hardware presets: group / device checks ──
    {
        let hw = &config.integration.hardware;
        if hw.joystick {
            if is_user_in_group(&env.username, "input") {
                check!("hardware: joystick", "pass", "user in 'input' group");
            } else {
                check!(
                    "hardware: joystick",
                    "warn",
                    "user not in 'input' group — joystick (/dev/input) will be denied"
                );
            }
        }
        if hw.webcam {
            if is_user_in_group(&env.username, "video") {
                check!("hardware: webcam", "pass", "user in 'video' group");
            } else {
                check!(
                    "hardware: webcam",
                    "warn",
                    "user not in 'video' group — /dev/video* will be denied"
                );
            }
        }
        if hw.serial {
            if is_user_in_group(&env.username, "dialout") || is_user_in_group(&env.username, "uucp")
            {
                check!("hardware: serial", "pass", "user in 'dialout'/'uucp'");
            } else {
                check!(
                    "hardware: serial",
                    "warn",
                    "user not in 'dialout' or 'uucp' — /dev/ttyUSB* will be denied"
                );
            }
        }
        if hw.kvm {
            let p = Path::new("/dev/kvm");
            if p.exists() {
                // Check read/write via metadata permissions or try open
                match std::fs::File::open(p) {
                    Ok(_) => check!("hardware: kvm", "pass", "/dev/kvm accessible"),
                    Err(e) => check!(
                        "hardware: kvm",
                        "warn",
                        format!("/dev/kvm exists but not accessible: {e}")
                    ),
                }
            } else {
                check!(
                    "hardware: kvm",
                    "warn",
                    "/dev/kvm not found — hardware virtualization unavailable"
                );
            }
        }
        if hw.yubikey {
            // yubikey uses pcscd socket + hidraw; no group check, just note
            check!(
                "hardware: yubikey",
                "pass",
                "yubikey preset enabled (pcscd + hidraw)"
            );
        }
    }

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
                check!(&label, "pass", "systemd credential passthrough");
            } else if available.contains(name) {
                check!(&label, "pass", "found in podman secret store");
            } else {
                check!(
                    &label,
                    "fail",
                    format!("missing — create with `podman secret create {name} -`")
                );
            }
        }
    }

    // ── config layout: legacy root configs ──
    {
        let legacy = podbox::config::find_legacy_root_configs();
        if !legacy.is_empty() {
            let names = legacy
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .collect::<Vec<_>>()
                .join(", ");
            if fix && confirm_fix("Migrate legacy configs from root to profiles/ directory?") {
                match crate::commands::migrate::run_migrate(crate::commands::migrate::MigrateOpts {
                    dry_run: false,
                    force: false,
                }) {
                    Ok(()) => check!(
                        "config layout",
                        "pass",
                        "migrated legacy configs to profiles/ directory"
                    ),
                    Err(e) => check!("config layout", "fail", format!("migration failed: {e}")),
                }
            } else {
                check!(
                    "config layout",
                    "warn",
                    format!(
                        "legacy configs found in root ({names}). Run `podbox migrate` (legacy path removed in next version)"
                    )
                );
            }
        } else {
            check!(
                "config layout",
                "pass",
                "using canonical ~/.config/podbox/profiles/"
            );
        }
    }

    // ── container memory: bare number without unit (e.g. "2" → "2G") ──
    {
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
                            Ok(()) => check!(
                                "memory",
                                "pass",
                                format!("fixed '{t}' → '{suggested}' via --fix")
                            ),
                            Err(e) => check!("memory", "fail", format!("fix failed: {e}")),
                        }
                    } else {
                        check!(
                            "memory",
                            "warn",
                            format!(
                                "memory = \"{t}\" has no unit; suggested \"{suggested}\" — run `podbox doctor --fix` to rewrite"
                            )
                        );
                    }
                } else {
                    check!(
                        "memory",
                        "warn",
                        format!(
                            "memory = \"{t}\" has no unit; suggested \"{suggested}\" — no config file found to rewrite"
                        )
                    );
                }
            } else if !podbox::config::validation::is_valid_memory(t) {
                check!(
                    "memory",
                    "fail",
                    format!("memory = \"{t}\" invalid (e.g. 2g, 512m)")
                );
            } else {
                check!("memory", "pass", t.to_string());
            }
        } else {
            check!("memory", "pass", "unlimited");
        }
    }

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
                    check!("guest version", "pass", gv);
                } else {
                    check!(
                        "guest version",
                        "warn",
                        format!(
                            "guest {gv} vs host {host_ver} — run `podbox build --rebuild {name}`"
                        )
                    );
                }
                if gp == host_proto {
                    check!("protocol", "pass", format!("v{gp}"));
                } else {
                    check!(
                        "protocol",
                        "warn",
                        format!("guest protocol v{gp} vs host v{host_proto} — rebuild")
                    );
                }
            }
            (Some(gv), None) => {
                if gv == host_ver {
                    check!("guest version", "pass", gv);
                } else {
                    check!(
                        "guest version",
                        "warn",
                        format!(
                            "guest {gv} vs host {host_ver} — run `podbox build --rebuild {name}`"
                        )
                    );
                }
                check!(
                    "protocol",
                    "warn",
                    "unknown guest protocol — rebuild".to_string()
                );
            }
            (None, Some(gp)) => {
                check!(
                    "guest version",
                    "warn",
                    "unknown guest version — rebuild".to_string()
                );
                if gp == host_proto {
                    check!("protocol", "pass", format!("v{gp}"));
                } else {
                    check!(
                        "protocol",
                        "warn",
                        format!("guest protocol v{gp} vs host v{host_proto} — rebuild")
                    );
                }
            }
            (None, None) => {
                if is_running {
                    check!(
                        "guest version",
                        "warn",
                        "guest not reporting version (old image) — rebuild".to_string()
                    );
                    check!("protocol", "warn", "unknown — rebuild".to_string());
                } else {
                    check!(
                        "guest version",
                        "warn",
                        "container not running — cannot query guest version (rebuild to ensure match)".to_string()
                    );
                    check!("protocol", "warn", "container not running".to_string());
                }
            }
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
                    check!(
                        "stale sockets",
                        "pass",
                        format!("removed {removed} via --fix")
                    );
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
                let listed: Vec<String> = stale.iter().map(|p| p.display().to_string()).collect();
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
                check!(
                    "dead exports",
                    "pass",
                    format!("removed {removed} via --fix")
                );
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
            // Header: which container this run is for, with active hint on default run.
            let active = podbox::config::read_active_context();
            let is_active = active.as_deref() == Some(config.container.name.as_str());
            if is_active {
                println!(
                    "Doctor — container '{}' (active context)\n",
                    config.container.name
                );
            } else {
                println!("Doctor — container '{}'\n", config.container.name);
            }
            // Grouped sections in stable order; container-specific at the bottom.
            for group in ["Host", "Integration", "Container"] {
                let section: Vec<_> = entries.iter().filter(|e| e.group == group).collect();
                if section.is_empty() {
                    continue;
                }
                println!("{}", group.if_supports_color(Stream::Stdout, |s| s.bold()));
                for entry in &section {
                    let tag = match entry.status.as_str() {
                        "pass" => "PASS"
                            .if_supports_color(Stream::Stdout, |s| s.green())
                            .to_string(),
                        "warn" => "WARN"
                            .if_supports_color(Stream::Stdout, |s| s.yellow())
                            .to_string(),
                        "fail" => "FAIL"
                            .if_supports_color(Stream::Stdout, |s| s.red())
                            .to_string(),
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

fn rewrite_memory_raw(path: &Path, suggested: &str) -> Result<()> {
    let content = std::fs::read_to_string(path)?;
    let mut value: toml::Value = content.parse()?;
    if let Some(tbl) = value.get_mut("container").and_then(|c| c.as_table_mut()) {
        tbl.insert("memory".into(), toml::Value::String(suggested.to_string()));
    } else {
        anyhow::bail!("container table not found in {}", path.display());
    }
    let out = toml::to_string_pretty(&value)?;
    std::fs::write(path, out)?;
    Ok(())
}

/// Attempt to fix a bare `memory = "2"` directly from the raw TOML file,
/// bypassing `Config::load` validation. Used when `podbox doctor --fix` is
/// invoked on an otherwise-invalid config. Returns true if a fix was applied.
pub fn try_fix_bare_memory_for_target(target_name: Option<&str>, fix: bool) -> Result<bool> {
    if !fix {
        return Ok(false);
    }
    let name = match target_name {
        Some(n) => n.to_string(),
        None => match podbox::config::read_active_context() {
            Some(n) => n,
            None => return Ok(false),
        },
    };
    let path = match podbox::config::find_config_path(&name) {
        Some(p) => p,
        None => return Ok(false),
    };
    let content = std::fs::read_to_string(&path)?;
    let value: toml::Value = match content.parse() {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    let mem = match value
        .get("container")
        .and_then(|c| c.get("memory"))
        .and_then(|m| m.as_str())
    {
        Some(m) => m.trim().to_string(),
        None => return Ok(false),
    };
    if !podbox::config::validation::is_bare_memory_digits(&mem) {
        return Ok(false);
    }
    let suggested = format!("{mem}G");
    if !confirm_fix(&format!(
        "Rewrite memory '{mem}' → '{suggested}' in {}?",
        path.display()
    )) {
        return Ok(false);
    }
    rewrite_memory_raw(&path, &suggested)?;
    println!("Fixed memory '{mem}' → '{suggested}' in {}", path.display());
    Ok(true)
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
