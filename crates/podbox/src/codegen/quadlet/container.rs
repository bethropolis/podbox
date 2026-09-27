//! `.container` Quadlet section emitters.
//!
//! Extracted verbatim from `quadlet.rs`; see `super` for the unit entry
//! points.

use std::path::PathBuf;

use crate::config::Config;
use crate::env::HostEnv;
use crate::xdg::ResolvedXdgDirs;

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"))
}

fn escape_systemd_specifiers(value: &str) -> String {
    value.replace('%', "%%")
}

pub(super) fn emit_unit(lines: &mut Vec<String>, config: &Config, name: &str) {
    lines.push("[Unit]".into());
    lines.push(format!("Description=podbox -- {name}"));
    lines.push(format!("Requires={name}.socket"));
    lines.push(format!("After={name}.socket"));
    for dep in &config.systemd.requires {
        lines.push(format!("Requires={dep}"));
    }
    for dep in &config.systemd.after {
        lines.push(format!("After={dep}"));
    }
    if config.use_dbus_proxy() {
        lines.push(format!("Requires={name}-proxy.service"));
        lines.push(format!("After={name}-proxy.service"));
    }
    if config.use_wayland_proxy() {
        lines.push(format!("Requires={name}-compositor.service"));
        lines.push(format!("After={name}-compositor.service"));
    }
    lines.push("StartLimitBurst=5".into());
    lines.push("StartLimitIntervalSec=30s".into());
    lines.push(String::new());
}

pub(super) fn emit_container_image(
    lines: &mut Vec<String>,
    config: &Config,
    name: &str,
    home_in_container: &str,
    env: &HostEnv,
) {
    lines.push("[Container]".into());
    if config.image.source().is_prebuilt() && config.image.packages.install.is_empty() {
        let ref_str = match config.image.source() {
            crate::config::ImageSource::Prebuilt { ref_str } => ref_str,
            _ => config.image.base.clone(),
        };
        lines.push(format!("Image={ref_str}"));
        lines.push(format!("Retry={}", config.image.pull_retry));
        lines.push(format!("RetryDelay={}", config.image.pull_retry_delay));
    } else {
        lines.push(format!(
            "Image=localhost/podbox-{}:latest",
            config.image.name
        ));
    }
    lines.push(format!("ContainerName={name}"));
    if let Some(ref mode) = config.security.userns {
        lines.push(format!("UserNS={mode}"));
    } else {
        lines.push("UserNS=keep-id".into());
    }
    lines.push("User=root".into());
    if config.security.security_label_disable {
        lines.push("SecurityLabelDisable=true".into());
    }
    if let Some(ref seccomp) = config.security.seccomp {
        lines.push(format!("SeccompProfile={seccomp}"));
    }
    if config.security.no_new_privileges {
        lines.push("NoNewPrivileges=true".into());
    }
    if let Some(ref mem) = config.container.memory {
        lines.push(format!("Memory={mem}"));
    }
    if let Some(ref cpus) = config.container.cpus {
        if let Ok(v) = cpus.parse::<f64>() {
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let quota = (v * 100_000.0) as u64;
            lines.push(format!("CpuQuota={quota}"));
        }
    }
    if config.security.read_only_rootfs {
        lines.push("ReadOnly=true".into());
    }
    if let Some(ref profile) = config.security.apparmor {
        lines.push(format!("AppArmor={profile}"));
    }
    lines.push(format!("Environment=HOME={home_in_container}"));
    lines.push(format!(
        "Environment=HOST_USER={}",
        escape_systemd_specifiers(&env.username)
    ));
    lines.push("Environment=HOST_UID=%U".into());
    lines.push("Environment=HOST_GID=%G".into());
    lines.push("Environment=PATH=/run/podbox/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into());
    lines.push(String::new());
}

pub(super) fn emit_network(lines: &mut Vec<String>, config: &Config) {
    let mode = config.network.effective_mode();
    lines.push(format!("Network={mode}"));
    if mode != "host" && mode != "none" {
        for port in &config.network.ports {
            lines.push(format!("PublishPort={port}"));
        }
    }
    lines.push(String::new());
}

pub(super) fn emit_volumes(
    lines: &mut Vec<String>,
    config: &Config,
    xdg: &ResolvedXdgDirs,
    env: &HostEnv,
    name: &str,
    home_in_container: &str,
) {
    // Isolated custom home
    let host_home = config.container.home.to_string_lossy().to_string();
    lines.push(format!("Volume={host_home}:{home_in_container}:Z",));
    lines.push(String::new());

    // Selective XDG dirs
    emit_xdg_dir(lines, "Documents", &xdg.documents, home_in_container);
    emit_xdg_dir(lines, "Downloads", &xdg.downloads, home_in_container);
    emit_xdg_dir(lines, "Pictures", &xdg.pictures, home_in_container);
    emit_xdg_dir(lines, "Music", &xdg.music, home_in_container);
    emit_xdg_dir(lines, "Videos", &xdg.videos, home_in_container);
    emit_xdg_dir(lines, "Desktop", &xdg.desktop, home_in_container);
    emit_xdg_dir(lines, "Projects", &xdg.projects, home_in_container);

    if xdg.documents.is_some()
        || xdg.downloads.is_some()
        || xdg.pictures.is_some()
        || xdg.music.is_some()
        || xdg.videos.is_some()
        || xdg.desktop.is_some()
        || xdg.projects.is_some()
    {
        lines.push(String::new());
    }

    // Visual integration: themes, fonts, icons
    if config.integration.sync_themes {
        let h = home();
        if h.join(".themes").exists() {
            lines.push(format!("Volume=%h/.themes:{home_in_container}/.themes:ro"));
        }
        if env.host_has_local_share_themes {
            lines.push(format!(
                "Volume=%h/.local/share/themes:{home_in_container}/.local/share/themes:ro"
            ));
        }
    }
    if config.integration.sync_icons {
        let h = home();
        if h.join(".icons").exists() {
            lines.push(format!("Volume=%h/.icons:{home_in_container}/.icons:ro"));
        }
        if env.host_has_local_share_icons {
            lines.push(format!(
                "Volume=%h/.local/share/icons:{home_in_container}/.local/share/icons:ro"
            ));
        }
    }
    if config.integration.sync_fonts {
        let h = home();
        if h.join(".fonts").exists() {
            lines.push(format!("Volume=%h/.fonts:{home_in_container}/.fonts:ro"));
        }
        if env.host_has_local_share_fonts {
            lines.push(format!(
                "Volume=%h/.local/share/fonts:{home_in_container}/.local/share/fonts:ro"
            ));
        }
    }
    if config.integration.sync_themes
        || config.integration.sync_icons
        || config.integration.sync_fonts
    {
        lines.push(String::new());
    }

    // Timezone sync
    if env.host_has_localtime {
        lines.push("Volume=/etc/localtime:/etc/localtime:ro".into());
    }
    if env.host_has_timezone_file {
        lines.push("Volume=/etc/timezone:/etc/timezone:ro".into());
    }
    if env.host_has_localtime || env.host_has_timezone_file {
        lines.push(String::new());
    }

    // The guest daemon needs XDG_RUNTIME_DIR to locate the host socket
    // regardless of Wayland/audio integration.
    lines.push("Environment=XDG_RUNTIME_DIR=%t".into());

    // Wayland
    if config.integration.wayland {
        if let Some(ref display) = env.wayland_display {
            lines.push(format!(
                "Environment=WAYLAND_DISPLAY={}",
                escape_systemd_specifiers(display)
            ));
            lines.push("Environment=MOZ_ENABLE_WAYLAND=1".into());
            if config.wayland.firewall {
                lines.push(format!(
                    "Volume=%t/podbox/{name}-wayland.sock:%t/{display}:ro"
                ));
            } else {
                lines.push(format!("Volume=%t/{display}:%t/{display}:ro"));
            }
            lines.push(String::new());
        }
    }

    // Audio (PipeWire + PulseAudio)
    if config.integration.audio {
        if env.pipewire_socket.is_some() {
            lines.push("Volume=%t/pipewire-0:%t/pipewire-0".into());
            lines.push("Environment=PIPEWIRE_RUNTIME_DIR=%t".into());
        }
        if env.pulse_dir.is_some() {
            lines.push("Volume=%t/pulse:%t/pulse".into());
            lines.push("Environment=PULSE_SERVER=unix:%t/pulse/native".into());
        }
        if env.pipewire_socket.is_some() || env.pulse_dir.is_some() {
            lines.push(String::new());
        }
    }

    // SSH agent
    if config.integration.ssh_agent {
        if let Some(ref sock) = env.ssh_agent_socket {
            lines.push(format!(
                "Volume={}:/run/podbox/ssh-agent.sock",
                sock.display()
            ));
            lines.push("Environment=SSH_AUTH_SOCK=/run/podbox/ssh-agent.sock".into());
        } else {
            eprintln!(
                "Warning: ssh_agent = true but SSH_AUTH_SOCK not found on host. Skipping SSH agent."
            );
        }
        lines.push(String::new());
    }

    // GPG agent
    if config.integration.gpg_agent {
        if let Some(ref sock) = env.gpg_agent_socket {
            lines.push(format!(
                "Volume={}:/run/podbox/gnupg/S.gpg-agent:ro",
                sock.display()
            ));
            lines.push("Environment=GPG_TTY=/dev/pts/0".into());
            lines.push("Environment=GNUPGHOME=/run/podbox/gnupg".into());
        } else {
            eprintln!(
                "Warning: gpg_agent = true but S.gpg-agent socket not found on host. Skipping GPG agent."
            );
        }
        lines.push(String::new());
    }

    // Sandbox environment detection marker (read-only host-side kernel mount)
    let flatpak_info_path = crate::build::build_context_dir(name).join(".flatpak-info");
    lines.push(format!(
        "Volume={}:/.flatpak-info:ro",
        flatpak_info_path.display()
    ));
    lines.push(String::new());

    // D-Bus
    if config.integration.dbus && env.dbus_socket.is_some() {
        if config.use_dbus_proxy() {
            lines.push(format!(
                "Volume=%t/podbox/{name}-dbus.sock:/run/podbox/dbus.sock:ro"
            ));
            lines.push(
                "Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/podbox/dbus.sock".into(),
            );
        } else {
            lines.push("Volume=%t/bus:%t/bus".into());
            lines.push("Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=%t/bus".into());
        }
        lines.push(String::new());
    }

    // Host-guest socket
    lines.push(format!(
        "Volume=%t/podbox/{name}.sock:%t/podbox/{name}.sock"
    ));
    lines.push(String::new());

    // Extra mounts
    for mount in &config.container.mounts.extra {
        lines.push(format!("Volume={mount}"));
    }
    if !config.container.mounts.extra.is_empty() {
        lines.push(String::new());
    }

    let caches = &config.storage.shared_caches;
    if caches.cargo {
        lines.push(format!(
            "Volume=podbox-cache-cargo-registry:{}:U",
            home_in_container_path("~/.cargo/registry", home_in_container)
        ));
        lines.push(format!(
            "Volume=podbox-cache-cargo-git:{}:U",
            home_in_container_path("~/.cargo/git", home_in_container)
        ));
    }
    for (enabled, name, path) in [
        (caches.npm, "npm", "~/.npm"),
        (caches.pnpm, "pnpm", "~/.local/share/pnpm/store"),
        (caches.pip, "pip", "~/.cache/pip"),
        (caches.ccache, "ccache", "~/.cache/ccache"),
        (caches.go, "go", "~/go/pkg/mod"),
        (caches.rustup, "rustup", "~/.rustup"),
    ] {
        if enabled {
            lines.push(format!(
                "Volume=podbox-cache-{name}:{}:U",
                home_in_container_path(path, home_in_container)
            ));
        }
    }
    for cache in &caches.custom {
        lines.push(format!(
            "Volume=podbox-cache-{}:{}:U",
            cache.name,
            home_in_container_path(&cache.container_path, home_in_container)
        ));
    }
    if caches.cargo
        || caches.npm
        || caches.pnpm
        || caches.pip
        || caches.ccache
        || caches.go
        || caches.rustup
        || !caches.custom.is_empty()
    {
        lines.push(String::new());
    }
}

fn home_in_container_path(path: &str, home: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => format!("{home}/{rest}"),
        None if path == "~" => home.to_string(),
        None => path.to_string(),
    }
}

pub(super) fn emit_env(lines: &mut Vec<String>, config: &Config, name: &str, _env: &HostEnv) {
    // Locale environment
    if let Some(ref locale) = _env.host_locale {
        let locale = escape_systemd_specifiers(locale);
        lines.push(format!("Environment=LANG={locale}"));
        lines.push(format!("Environment=LC_ALL={locale}"));
        lines.push(format!("Environment=LC_CTYPE={locale}"));
        lines.push(String::new());
    }

    // Extra user env
    for (key, value) in &config.container.env.values {
        if key.chars().all(|c| c.is_alphanumeric() || c == '_') {
            // systemd expands % specifiers in Environment= values. Double
            // user-supplied percent signs so they arrive literally in the
            // container (e.g. `date +%s` must not become `%s` expansion).
            let clean = escape_systemd_specifiers(value)
                .replace('\n', " ")
                .replace('\r', "");
            let escaped = clean.replace('\\', "\\\\").replace('"', "\\\"");
            let env_val = if escaped.contains(' ') || escaped.is_empty() {
                format!("\"{escaped}\"")
            } else {
                escaped
            };
            lines.push(format!("Environment={key}={env_val}"));
        } else {
            eprintln!("Warning: ignoring invalid environment variable key '{key}'");
        }
    }
    if !config.container.services.is_empty() {
        let payload =
            serde_json::to_string(&config.container.services).unwrap_or_else(|_| "{}".into());
        let escaped = escape_systemd_specifiers(&payload)
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        lines.push(format!("Environment=PODBOX_SERVICES_JSON=\"{escaped}\""));
    }
    lines.push(format!(
        "Environment=PODBOX_CONTAINER={}",
        escape_systemd_specifiers(name)
    ));
    lines.push(String::new());
}

pub(super) fn emit_auto_update(lines: &mut Vec<String>, config: &Config) {
    if config.lifecycle.auto_update {
        if config.image.source().is_prebuilt() {
            lines.push("AutoUpdate=registry".into());
        } else {
            lines.push("AutoUpdate=local".into());
        }
        lines.push(String::new());
    }
}

pub(super) fn emit_podman_args(lines: &mut Vec<String>, config: &Config) {
    lines.push("PodmanArgs=--init".into());
    lines.push("PodmanArgs=--workdir=/home/%u".into());
    let cap_preset = config.security.cap_preset;
    let has_any_cap = !cap_preset.caps().is_empty() || !config.security.cap_add.is_empty();
    for cap in cap_preset.caps() {
        lines.push(format!("PodmanArgs=--cap-add={cap}"));
    }
    for cap in &config.security.cap_add {
        lines.push(format!("PodmanArgs=--cap-add={cap}"));
    }
    if has_any_cap {
        lines.push(String::new());
    }
    if let Some(ref cmd) = config.container.reload_cmd {
        lines.push(format!("ReloadCmd={cmd}"));
        lines.push(String::new());
    }
}

pub(super) fn emit_service_section(lines: &mut Vec<String>, config: &Config) {
    lines.push("[Service]".into());
    lines.push(format!("Slice={}", config.container.slice));
    lines.push(format!("CPUWeight={}", config.container.cpu_weight));
    lines.push("Restart=on-failure".into());
    lines.push("RestartSec=2s".into());
    if config.lifecycle.on_stop == crate::config::OnStop::Remove {
        lines.push("AutoRemove=true".into());
    }
    lines.push(String::new());
}

pub(super) fn emit_install_section(lines: &mut Vec<String>, config: &Config) {
    lines.push("[Install]".into());
    if config.lifecycle.autostart {
        lines.push("WantedBy=default.target".into());
    }
}

pub(super) fn emit_xdg_dir(
    lines: &mut Vec<String>,
    dir_name: &str,
    xdg_dir: &Option<crate::xdg::ResolvedXdgDir>,
    container_home: &str,
) {
    if let Some(resolved) = xdg_dir {
        let mode = if resolved.read_write { "z" } else { "ro,z" };
        lines.push(format!(
            "Volume={}:{container_home}/{dir_name}:{mode}",
            resolved.path.display()
        ));
    }
}
