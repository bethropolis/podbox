use super::container_core::wayland_socket_path;
use super::{caches, value};
use crate::config::Config;
use crate::env::HostEnv;
use crate::xdg::ResolvedXdgDirs;

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

    // Timezone sync: `Timezone=local` (podman's `--tz=local`) sets up
    // /etc/localtime as a proper symlink, so zone-name lookups via
    // readlink keep working and host changes propagate. A bind-mounted
    // host file would pin the inode until the container restarts.
    if env.host_has_localtime {
        lines.push("Timezone=local".into());
    }
    if env.host_has_timezone_file {
        lines.push("Volume=/etc/timezone:/etc/timezone:ro".into());
    }
    if env.host_has_localtime || env.host_has_timezone_file {
        lines.push(String::new());
    }

    // Host terminal capability definitions. Mounted over the image's system
    // database because ncurses ignores TERMINFO_DIRS for root; the host's
    // database is authoritative for the terminal the user is actually on.
    if let Some(ref terminfo) = env.host_terminfo_dir
        && !config
            .container
            .mounts
            .extra
            .iter()
            .any(|mount| mount.split(':').nth(1) == Some("/usr/share/terminfo"))
    {
        match value::raw(&terminfo.display().to_string()) {
            Ok(escaped) => {
                value::push_composed(lines, format!("Volume={escaped}:/usr/share/terminfo:ro"))
            }
            Err(e) => warnings.push(format!("skipping terminfo Volume= line: {e}")),
        }
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
                Ok(escaped) => value::push_composed(
                    lines,
                    format!(
                        "Volume={}:%t/{escaped}:ro",
                        wayland_socket_path(config, display, name),
                    ),
                ),
                Err(e) => warnings.push(format!("skipping Wayland Volume= line: {e}")),
            }
            lines.push(String::new());
        }
    }

    if config.integration.x11 {
        value::push_composed(
            lines,
            format!("Volume=%t/podbox/{name}-x11:/tmp/.X11-unix:ro"),
        );
        lines.push(String::new());
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

fn emit_xdg_dir(
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
