//! `.container` Quadlet section emitters.
//!
//! Extracted verbatim from `quadlet.rs`; see `super` for the unit entry
//! points.

use super::caches;
use super::value;
use crate::config::Config;
use crate::env::HostEnv;
use crate::xdg::ResolvedXdgDirs;

pub(super) fn emit_quadlet(lines: &mut Vec<String>) {
    lines.push("[Quadlet]".into());
    lines.push("DefaultDependencies=false".into());
    lines.push(String::new());
}

pub(super) fn emit_unit(
    lines: &mut Vec<String>,
    config: &Config,
    name: &str,
    warnings: &mut Vec<String>,
) {
    lines.push("[Unit]".into());
    value::push_raw(lines, warnings, "Description", &format!("podbox -- {name}"));
    value::push_raw(lines, warnings, "Requires", &format!("{name}.socket"));
    value::push_raw(lines, warnings, "After", &format!("{name}.socket"));
    for dep in &config.systemd.requires {
        value::push_raw(lines, warnings, "Requires", dep);
    }
    for dep in &config.systemd.after {
        value::push_raw(lines, warnings, "After", dep);
    }
    if config.use_dbus_proxy() {
        value::push_raw(
            lines,
            warnings,
            "Requires",
            &format!("{name}-proxy.service"),
        );
        value::push_raw(lines, warnings, "After", &format!("{name}-proxy.service"));
    }
    if config.use_wayland_proxy() {
        value::push_raw(
            lines,
            warnings,
            "Requires",
            &format!("{name}-compositor.service"),
        );
        value::push_raw(
            lines,
            warnings,
            "After",
            &format!("{name}-compositor.service"),
        );
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
    warnings: &mut Vec<String>,
) {
    lines.push("[Container]".into());
    if config.image.source().is_prebuilt()
        && !crate::codegen::containerfile::prebuilt_overlay_needed(config)
    {
        let ref_str = match config.image.source() {
            crate::config::ImageSource::Prebuilt { ref_str } => ref_str,
            _ => config.image.base.clone(),
        };
        value::push_raw(lines, warnings, "Image", &ref_str);
        value::push_raw(
            lines,
            warnings,
            "Retry",
            &config.image.pull_retry.to_string(),
        );
        value::push_raw(
            lines,
            warnings,
            "RetryDelay",
            &config.image.pull_retry_delay,
        );
    } else {
        value::push_raw(lines, warnings, "Image", &config.local_image_tag());
    }
    value::push_raw(lines, warnings, "ContainerName", name);
    if let Some(ref mode) = config.security.userns {
        value::push_raw(lines, warnings, "UserNS", mode);
    } else {
        lines.push("UserNS=keep-id".into());
    }
    lines.push("User=root".into());
    if config.security.security_label_disable {
        lines.push("SecurityLabelDisable=true".into());
    }
    if let Some(ref seccomp) = config.security.seccomp {
        value::push_raw(lines, warnings, "SeccompProfile", seccomp);
    }
    if config.security.no_new_privileges {
        lines.push("NoNewPrivileges=true".into());
    }
    if let Some(ref mem) = config.container.memory {
        value::push_raw(lines, warnings, "Memory", mem);
    }
    if let Some(ref cpus) = config.container.cpus {
        // Quadlet has no `CpuQuota` key: the quadlet-generator rejects the
        // whole unit as "unsupported key", so the container silently never
        // starts. CPU limits go through `--cpus`, which also takes fractions
        // (`0.5`) that a period-based quota cannot express.
        if let Ok(v) = cpus.parse::<f64>() {
            value::push_raw(lines, warnings, "PodmanArgs", &format!("--cpus={v}"));
        }
    }
    if config.security.read_only_rootfs {
        lines.push("ReadOnly=true".into());
        // A read-only root leaves no writable runtime paths, and the guest
        // daemon needs some: it does `create_dir_all("/run/podbox/bin")?`
        // before it can talk to the host at all, so without a tmpfs there the
        // image never starts. Ordinary tools need /tmp and /var/tmp too.
        // tmpfs keeps the image layers immutable while the running container
        // still has somewhere to work.
        lines.push("Mount=type=tmpfs,destination=/run,tmpfs-mode=1777".into());
        lines.push("Mount=type=tmpfs,destination=/tmp,tmpfs-mode=1777".into());
        lines.push("Mount=type=tmpfs,destination=/var/tmp,tmpfs-mode=1777".into());
    }
    if let Some(ref profile) = config.security.apparmor {
        value::push_raw(lines, warnings, "AppArmor", profile);
    }
    value::push_composed(lines, format!("Environment=HOME={home_in_container}"));
    value::push_env(lines, warnings, "HOST_USER", &env.username);
    lines.push("Environment=HOST_UID=%U".into());
    lines.push("Environment=HOST_GID=%G".into());
    lines.push("Environment=PATH=/run/podbox/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into());
    lines.push(String::new());
}

pub(super) fn emit_network(lines: &mut Vec<String>, config: &Config, warnings: &mut Vec<String>) {
    let mode = config.network.effective_mode();
    value::push_raw(lines, warnings, "Network", mode);
    if mode != "host" && mode != "none" {
        for port in &config.network.ports {
            value::push_raw(lines, warnings, "PublishPort", port);
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
    warnings: &mut Vec<String>,
) {
    // Isolated custom home
    let host_home = config.container.home.to_string_lossy().to_string();
    match value::raw(&host_home) {
        Ok(escaped) => {
            value::push_composed(lines, format!("Volume={escaped}:{home_in_container}:Z"))
        }
        Err(e) => warnings.push(format!("skipping home Volume= line: {e}")),
    }
    lines.push(String::new());

    // Selective XDG dirs
    emit_xdg_dir(
        lines,
        warnings,
        "Documents",
        &xdg.documents,
        home_in_container,
    );
    emit_xdg_dir(
        lines,
        warnings,
        "Downloads",
        &xdg.downloads,
        home_in_container,
    );
    emit_xdg_dir(
        lines,
        warnings,
        "Pictures",
        &xdg.pictures,
        home_in_container,
    );
    emit_xdg_dir(lines, warnings, "Music", &xdg.music, home_in_container);
    emit_xdg_dir(lines, warnings, "Videos", &xdg.videos, home_in_container);
    emit_xdg_dir(lines, warnings, "Desktop", &xdg.desktop, home_in_container);
    emit_xdg_dir(
        lines,
        warnings,
        "Projects",
        &xdg.projects,
        home_in_container,
    );

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

    // Visual integration: themes, fonts, icons.
    // Presence comes from HostEnv (resolved once on the host, mocked for
    // previews) — never probe the filesystem here so output stays a pure
    // function of (config, env, xdg).
    if config.integration.sync_themes {
        if env.host_has_themes {
            value::push_composed(
                lines,
                format!("Volume=%h/.themes:{home_in_container}/.themes:ro"),
            );
        }
        if env.host_has_local_share_themes {
            value::push_composed(
                lines,
                format!("Volume=%h/.local/share/themes:{home_in_container}/.local/share/themes:ro"),
            );
        }
    }
    if config.integration.sync_icons {
        if env.host_has_icons {
            value::push_composed(
                lines,
                format!("Volume=%h/.icons:{home_in_container}/.icons:ro"),
            );
        }
        if env.host_has_local_share_icons {
            value::push_composed(
                lines,
                format!("Volume=%h/.local/share/icons:{home_in_container}/.local/share/icons:ro"),
            );
        }
    }
    if config.integration.sync_fonts {
        if env.host_has_fonts {
            value::push_composed(
                lines,
                format!("Volume=%h/.fonts:{home_in_container}/.fonts:ro"),
            );
        }
        if env.host_has_local_share_fonts {
            value::push_composed(
                lines,
                format!("Volume=%h/.local/share/fonts:{home_in_container}/.local/share/fonts:ro"),
            );
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
            value::push_env(lines, warnings, "WAYLAND_DISPLAY", display);
            lines.push("Environment=MOZ_ENABLE_WAYLAND=1".into());
            match value::raw(display) {
                Ok(escaped) => {
                    if config.wayland.firewall {
                        value::push_composed(
                            lines,
                            format!("Volume=%t/podbox/{name}-wayland.sock:%t/{escaped}:ro"),
                        );
                    } else {
                        value::push_composed(lines, format!("Volume=%t/{escaped}:%t/{escaped}:ro"));
                    }
                }
                Err(e) => warnings.push(format!("skipping Wayland Volume= line: {e}")),
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
            match value::raw(&sock.display().to_string()) {
                Ok(escaped) => value::push_composed(
                    lines,
                    format!("Volume={escaped}:/run/podbox/ssh-agent.sock"),
                ),
                Err(e) => warnings.push(format!("skipping SSH agent Volume= line: {e}")),
            }
            lines.push("Environment=SSH_AUTH_SOCK=/run/podbox/ssh-agent.sock".into());
        } else {
            warnings.push(
                "ssh_agent = true but SSH_AUTH_SOCK not found on host. Skipping SSH agent.".into(),
            );
        }
        lines.push(String::new());
    }

    // GPG agent
    if config.integration.gpg_agent {
        if let Some(ref sock) = env.gpg_agent_socket {
            match value::raw(&sock.display().to_string()) {
                Ok(escaped) => value::push_composed(
                    lines,
                    format!("Volume={escaped}:/run/podbox/gnupg/S.gpg-agent:ro"),
                ),
                Err(e) => warnings.push(format!("skipping GPG agent Volume= line: {e}")),
            }
            lines.push("Environment=GPG_TTY=/dev/pts/0".into());
            lines.push("Environment=GNUPGHOME=/run/podbox/gnupg".into());
        } else {
            warnings.push(
                "gpg_agent = true but S.gpg-agent socket not found on host. Skipping GPG agent."
                    .into(),
            );
        }
        lines.push(String::new());
    }

    // Sandbox environment detection marker (read-only host-side kernel mount)
    let flatpak_info_path = crate::env::build_context_dir(name).join(".flatpak-info");
    match value::raw(&flatpak_info_path.display().to_string()) {
        Ok(escaped) => {
            value::push_composed(lines, format!("Volume={escaped}:/.flatpak-info:ro"));
        }
        Err(e) => warnings.push(format!("skipping .flatpak-info Volume= line: {e}")),
    }
    lines.push(String::new());

    // D-Bus
    if config.integration.dbus && env.dbus_socket.is_some() {
        if config.use_dbus_proxy() {
            value::push_composed(
                lines,
                format!("Volume=%t/podbox/{name}-dbus.sock:/run/podbox/dbus.sock:ro"),
            );
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
    value::push_composed(
        lines,
        format!("Volume=%t/podbox/{name}.sock:%t/podbox/{name}.sock"),
    );
    lines.push(String::new());

    // Extra mounts
    for mount in &config.container.mounts.extra {
        value::push_word(lines, warnings, "Volume", mount);
    }
    if !config.container.mounts.extra.is_empty() {
        lines.push(String::new());
    }

    caches::emit_shared_caches(
        lines,
        &config.storage.shared_caches,
        home_in_container,
        warnings,
    );
    caches::emit_host_caches(
        lines,
        &config.storage.host_caches,
        home_in_container,
        env,
        warnings,
    );
    if caches::any_configured(config) {
        lines.push(String::new());
    }
}

pub(super) fn emit_env(
    lines: &mut Vec<String>,
    config: &Config,
    name: &str,
    _env: &HostEnv,
    warnings: &mut Vec<String>,
) {
    // Locale environment
    if let Some(ref locale) = _env.host_locale {
        value::push_env(lines, warnings, "LANG", locale);
        value::push_env(lines, warnings, "LC_ALL", locale);
        value::push_env(lines, warnings, "LC_CTYPE", locale);
        lines.push(String::new());
    }

    // Extra user env, layered over engine-derived defaults (SHELL and
    // friends — see ContainerConfig::effective_env). Explicit wins.
    for (key, value) in &config.container.effective_env() {
        if key.chars().all(|c| c.is_alphanumeric() || c == '_') {
            value::push_env(lines, warnings, key, value);
        } else {
            warnings.push(format!("ignoring invalid environment variable key '{key}'"));
        }
    }
    if !config.container.services.is_empty() {
        let payload =
            serde_json::to_string(&config.container.services).unwrap_or_else(|_| "{}".into());
        match value::word(&payload) {
            Ok(escaped) => {
                value::push_composed(lines, format!("Environment=PODBOX_SERVICES_JSON={escaped}"))
            }
            Err(e) => warnings.push(format!("skipping PODBOX_SERVICES_JSON line: {e}")),
        }
    }
    value::push_env(lines, warnings, "PODBOX_CONTAINER", name);
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

pub(super) fn emit_podman_args(
    lines: &mut Vec<String>,
    config: &Config,
    warnings: &mut Vec<String>,
) {
    lines.push("PodmanArgs=--init".into());
    lines.push("PodmanArgs=--workdir=/home/%u".into());
    let cap_preset = config.security.cap_preset;
    let has_any_cap = !cap_preset.caps().is_empty() || !config.security.cap_add.is_empty();
    for cap in cap_preset.caps() {
        value::push_raw(lines, warnings, "PodmanArgs", &format!("--cap-add={cap}"));
    }
    for cap in &config.security.cap_add {
        value::push_raw(lines, warnings, "PodmanArgs", &format!("--cap-add={cap}"));
    }
    if has_any_cap {
        lines.push(String::new());
    }
    if let Some(ref cmd) = config.container.reload_cmd {
        value::push_raw(lines, warnings, "ReloadCmd", cmd);
        lines.push(String::new());
    }
}

pub(super) fn emit_service_section(
    lines: &mut Vec<String>,
    config: &Config,
    warnings: &mut Vec<String>,
) {
    lines.push("[Service]".into());
    value::push_raw(lines, warnings, "Slice", &config.container.slice);
    value::push_raw(
        lines,
        warnings,
        "CPUWeight",
        &config.container.cpu_weight.to_string(),
    );
    lines.push("Restart=on-failure".into());
    lines.push("RestartSec=2s".into());
    emit_service_credentials(lines, config, warnings);
    if config.lifecycle.on_stop == crate::config::OnStop::Remove {
        lines.push("AutoRemove=true".into());
    }
    lines.push(String::new());
}

/// `LoadCredential=` lines for systemd-credential secrets.
///
/// Without these, `%d` (`$CREDENTIALS_DIRECTORY`) is empty and the
/// `Volume=%d/…` lines in `[Container]` mount nothing. The host-side `path`
/// is required by validation; `encrypted` selects the encrypted variant.
fn emit_service_credentials(lines: &mut Vec<String>, config: &Config, warnings: &mut Vec<String>) {
    use crate::config::{SecretEntry, SecretSource};
    for secret in &config.security.secrets {
        let (name, path, encrypted) = match secret {
            SecretEntry::Simple(_) => continue,
            SecretEntry::Detailed {
                name,
                source,
                path,
                encrypted,
                ..
            } => {
                if *source != SecretSource::Systemd {
                    continue;
                }
                (name, path, encrypted)
            }
        };
        match (value::raw(name), path.as_deref().map(value::raw)) {
            (Ok(name_escaped), Some(Ok(path_escaped))) => {
                let key = if *encrypted {
                    "LoadCredentialEncrypted"
                } else {
                    "LoadCredential"
                };
                value::push_composed(lines, format!("{key}={name_escaped}:{path_escaped}"));
            }
            _ => warnings.push(format!(
                "skipping credential '{name}': invalid name or path"
            )),
        }
    }
}

pub(super) fn emit_install_section(lines: &mut Vec<String>, config: &Config) {
    lines.push("[Install]".into());
    if config.lifecycle.autostart {
        lines.push("WantedBy=default.target".into());
    }
}

pub(super) fn emit_xdg_dir(
    lines: &mut Vec<String>,
    warnings: &mut Vec<String>,
    dir_name: &str,
    xdg_dir: &Option<crate::xdg::ResolvedXdgDir>,
    container_home: &str,
) {
    if let Some(resolved) = xdg_dir {
        let mode = if resolved.read_write { "z" } else { "ro,z" };
        match value::raw(&resolved.path.display().to_string()) {
            Ok(escaped) => value::push_composed(
                lines,
                format!("Volume={escaped}:{container_home}/{dir_name}:{mode}"),
            ),
            Err(e) => warnings.push(format!("skipping XDG {dir_name} Volume= line: {e}")),
        }
    }
}
