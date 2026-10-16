//! Shared helpers for the `codegen_*` integration test binaries.
//!
//! Each test binary pulls in only the helpers it uses, so helpers that a
//! given binary does not call would otherwise trip `dead_code` under
//! `-D warnings`.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Mutex;

use podbox::config::Config;
use podbox::env::HostEnv;
use podbox::xdg::ResolvedXdgDir;
use podbox::xdg::ResolvedXdgDirs;

/// Serializes tests that need to set `HOME` — `set_var` is process-global,
/// so parallel modification would corrupt results. Poisoning is tolerated:
/// a snapshot failure panics while holding the lock, and that must not
/// cascade into unrelated tests.
pub static HOME_LOCK: Mutex<()> = Mutex::new(());

/// Pin `HOME` (and the XDG base dirs) to fixed values while `f` runs.
///
/// Snapshot tests need deterministic `dirs` output: the real `HOME` varies
/// by machine and sibling tests swap it for a temp dir in parallel. Unlike
/// [`with_sandbox_home`], no directory is created — codegen only builds
/// paths, never touches the filesystem — so the fixed path need not exist.
/// `XDG_DATA_HOME` is pinned as well because `dirs` prefers it over `HOME`.
pub fn with_fixed_home(home: &str, f: impl FnOnce()) {
    let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let vars = ["HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME"];
    let old: Vec<(String, Option<std::ffi::OsString>)> = vars
        .iter()
        .map(|v| (v.to_string(), std::env::var_os(v)))
        .collect();
    // SAFETY: serialized behind HOME_LOCK like `with_sandbox_home`.
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var("HOME", home);
        std::env::set_var("XDG_DATA_HOME", format!("{home}/.local/share"));
        std::env::set_var("XDG_CACHE_HOME", format!("{home}/.cache"));
        std::env::remove_var("XDG_CONFIG_HOME");
    }
    f();
    // SAFETY: Same serialization as above.
    #[allow(unsafe_code)]
    unsafe {
        for (k, v) in old {
            if let Some(val) = v {
                std::env::set_var(k, val);
            } else {
                std::env::remove_var(k);
            }
        }
    }
}

/// Create a sandboxed home directory and run `f` with `HOME` pointing at it.
/// The temp dir is cleaned up on drop after restoring `HOME`.
pub fn with_sandbox_home(f: impl FnOnce(&std::path::Path)) {
    let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let old_home = std::env::var_os("HOME");
    // SAFETY: `set_var` is edition-2024 unsafe against multi-threaded env
    // mutation; this helper serializes all HOME mutations behind HOME_LOCK
    // and restores the prior value on exit. No library alternative exists.
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var("HOME", tmp.path());
    }
    // `dirs` prefers XDG_CONFIG_HOME over HOME, so a developer machine with
    // it set would leak real configs into the sandbox. Clear it the same way.
    let old_xdg = std::env::var_os("XDG_CONFIG_HOME");
    // SAFETY: Same serialization as HOME above.
    #[allow(unsafe_code)]
    unsafe {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
    f(tmp.path());
    // SAFETY: Same serialization as above.
    #[allow(unsafe_code)]
    unsafe {
        if let Some(h) = old_home {
            std::env::set_var("HOME", h);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(x) = old_xdg {
            std::env::set_var("XDG_CONFIG_HOME", x);
        }
    }
    // _guard dropped → HOME_LOCK released.
    // TempDir is dropped here — cleaned up automatically.
}

pub fn load_config(name: &str) -> Config {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let content = std::fs::read_to_string(path).unwrap();
    Config::parse(&content).unwrap()
}

pub fn default_env() -> HostEnv {
    HostEnv {
        uid: 1000,
        username: "testuser".into(),
        home_dir: PathBuf::from("/home/testuser"),
        xdg_runtime_dir: PathBuf::from("/run/user/1000"),
        wayland_display: Some("wayland-0".into()),
        wayland_socket: Some(PathBuf::from("/run/user/1000/wayland-0")),
        pipewire_socket: Some(PathBuf::from("/run/user/1000/pipewire-0")),
        pulse_dir: Some(PathBuf::from("/run/user/1000/pulse")),
        dbus_socket: Some(PathBuf::from("/run/user/1000/bus")),
        gpu_has_dri: false,
        gpu_has_nvidia: false,
        gpu_has_nvidia_uvm: false,
        host_has_localtime: false,
        host_has_timezone_file: false,
        host_has_themes: false,
        host_has_icons: false,
        host_has_fonts: false,
        host_has_local_share_themes: false,
        host_has_local_share_icons: false,
        host_has_local_share_fonts: false,
        host_shell: None,
        host_locale: None,
        gpg_agent_socket: None,
        gpg_home: None,
        ssh_agent_socket: None,
        host_cache_dirs: None,
    }
}

pub fn env_with_ssh_agent(sock: Option<PathBuf>) -> HostEnv {
    let mut env = default_env();
    env.ssh_agent_socket = sock;
    env
}

pub fn default_xdg() -> ResolvedXdgDirs {
    ResolvedXdgDirs {
        documents: Some(ResolvedXdgDir {
            path: PathBuf::from("/home/user/Documents"),
            read_write: false,
        }),
        downloads: Some(ResolvedXdgDir {
            path: PathBuf::from("/home/user/Downloads"),
            read_write: false,
        }),
        pictures: None,
        music: None,
        videos: None,
        desktop: None,
        projects: None,
    }
}

// ---- Containerfile tests ----
