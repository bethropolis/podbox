#[cfg(feature = "cli")]
use std::env;
#[cfg(feature = "cli")]
use std::path::Path;
use std::path::PathBuf;

#[cfg(feature = "cli")]
use anyhow::Result;
#[cfg(feature = "cli")]
use nix::unistd::getuid;

/// Resolved host environment for socket and path detection.
pub struct HostEnv {
    pub uid: u32,
    pub username: String,
    pub home_dir: PathBuf,
    pub xdg_runtime_dir: PathBuf,
    pub wayland_display: Option<String>,
    pub wayland_socket: Option<PathBuf>,
    pub pipewire_socket: Option<PathBuf>,
    pub pulse_dir: Option<PathBuf>,
    pub dbus_socket: Option<PathBuf>,
    pub gpu_has_dri: bool,
    pub gpu_has_nvidia: bool,
    pub gpu_has_nvidia_uvm: bool,
    pub host_has_localtime: bool,
    pub host_has_timezone_file: bool,
    pub host_has_themes: bool,
    pub host_has_icons: bool,
    pub host_has_fonts: bool,
    pub host_has_local_share_themes: bool,
    pub host_has_local_share_icons: bool,
    pub host_has_local_share_fonts: bool,
    pub host_shell: Option<String>,
    pub host_locale: Option<String>,
    pub gpg_agent_socket: Option<PathBuf>,
    pub gpg_home: Option<PathBuf>,
    pub ssh_agent_socket: Option<PathBuf>,
}

impl HostEnv {
    /// Deterministic stand-in host for previews (Studio/wasm): a generic
    /// Linux desktop with everything present, so generated units show the
    /// full integration surface instead of an empty host.
    pub fn mock_preview() -> Self {
        Self {
            uid: 1000,
            username: "user".into(),
            home_dir: PathBuf::from("/home/user"),
            xdg_runtime_dir: PathBuf::from("/run/user/1000"),
            wayland_display: Some("wayland-0".into()),
            wayland_socket: Some(PathBuf::from("/run/user/1000/wayland-0")),
            pipewire_socket: Some(PathBuf::from("/run/user/1000/pipewire-0")),
            pulse_dir: Some(PathBuf::from("/run/user/1000/pulse")),
            dbus_socket: Some(PathBuf::from("/run/user/1000/bus")),
            gpu_has_dri: true,
            gpu_has_nvidia: false,
            gpu_has_nvidia_uvm: false,
            host_has_localtime: true,
            host_has_timezone_file: false,
            host_has_themes: true,
            host_has_icons: true,
            host_has_fonts: true,
            host_has_local_share_themes: true,
            host_has_local_share_icons: true,
            host_has_local_share_fonts: true,
            host_shell: Some("/usr/bin/fish".into()),
            host_locale: Some("en_US.UTF-8".into()),
            gpg_agent_socket: None,
            gpg_home: None,
            ssh_agent_socket: None,
        }
    }
}

/// Build context directory: ~/.local/share/podbox/<name>/
/// Pure path computation (no host probing), so it lives here rather than in
/// the `cli`-gated `build` module — `codegen` needs it for the
/// `.flatpak-info` marker mount.
pub fn build_context_dir(name: &str) -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("podbox")
        .join(name)
}

/// Resolve the host environment.
///
/// Reads `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR` from the environment.
/// Detects presence of Wayland, PipeWire, PulseAudio, and D-Bus sockets.
/// Detects timezone files and modern XDG theme/icon/font directories.
///
/// Host-only (`cli` feature): probes uids, sockets, and device nodes that
/// don't exist on wasm. WASM callers use [`HostEnv::mock_preview`] instead.
#[cfg(feature = "cli")]
pub fn resolve() -> Result<HostEnv> {
    let uid = getuid().as_raw();
    let username = env::var("USER")
        .or_else(|_| env::var("LOGNAME"))
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid))
                .ok()
                .flatten()
                .map(|u| u.name)
                .unwrap_or_else(|| "user".into())
        });

    let xdg_runtime_dir: PathBuf = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{uid}")));

    let wayland_display = env::var("WAYLAND_DISPLAY").ok();
    let wayland_socket = wayland_display
        .as_ref()
        .map(|d| xdg_runtime_dir.join(d))
        .filter(|p| p.exists());

    let pipewire_socket = Some(xdg_runtime_dir.join("pipewire-0")).filter(|p| p.exists());

    let pulse_dir = Some(xdg_runtime_dir.join("pulse")).filter(|p| p.join("native").exists());

    let dbus_socket = Some(xdg_runtime_dir.join("bus")).filter(|p| p.exists());

    let gpu_has_dri = Path::new("/dev/dri").exists();
    let gpu_has_nvidia = Path::new("/dev/nvidiactl").exists();
    let gpu_has_nvidia_uvm = Path::new("/dev/nvidia-uvm").exists();

    let host_has_localtime = Path::new("/etc/localtime").exists();
    let host_has_timezone_file = Path::new("/etc/timezone").exists();

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"));
    let host_has_themes = home.join(".themes").exists();
    let host_has_icons = home.join(".icons").exists();
    let host_has_fonts = home.join(".fonts").exists();
    let host_has_local_share_themes = home.join(".local/share/themes").exists();
    let host_has_local_share_icons = home.join(".local/share/icons").exists();
    let host_has_local_share_fonts = home.join(".local/share/fonts").exists();

    let host_shell = env::var("SHELL").ok().filter(|s| !s.is_empty());
    let host_locale = env::var("LANG")
        .ok()
        .or_else(|| env::var("LC_ALL").ok())
        .or_else(|| env::var("LC_CTYPE").ok())
        .filter(|s| !s.is_empty());

    let gpg_home = env::var("GNUPGHOME").ok().map(PathBuf::from).or_else(|| {
        let fallback = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/root"))
            .join(".gnupg");
        if fallback.exists() {
            Some(fallback)
        } else {
            None
        }
    });
    let gpg_agent_socket = gpg_home.as_ref().and_then(|gpg| {
        let sock = gpg.join("S.gpg-agent");
        if sock.exists() { Some(sock) } else { None }
    });

    let ssh_agent_socket = env::var_os("SSH_AUTH_SOCK")
        .map(PathBuf::from)
        .filter(|p| p.exists());

    Ok(HostEnv {
        uid,
        username,
        home_dir: home,
        xdg_runtime_dir,
        wayland_display,
        wayland_socket,
        pipewire_socket,
        pulse_dir,
        dbus_socket,
        gpu_has_dri,
        gpu_has_nvidia,
        gpu_has_nvidia_uvm,
        host_has_localtime,
        host_has_timezone_file,
        host_has_themes,
        host_has_icons,
        host_has_fonts,
        host_has_local_share_themes,
        host_has_local_share_icons,
        host_has_local_share_fonts,
        host_shell,
        host_locale,
        gpg_agent_socket,
        gpg_home,
        ssh_agent_socket,
    })
}
