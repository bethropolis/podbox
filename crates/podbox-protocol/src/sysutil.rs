//! Unix-only process helpers both the host CLI and the guest daemon need.
//!
//! Small, dependency-light implementations shared so each of them exists
//! exactly once: the built-in guest cannot be rebuilt for users, but its
//! behaviour tracks the host whenever the helpers agree. Keep this module
//! free of podbox types — it only touches the standard library plus the
//! `nix`/`rustix`/`signal-hook` shims, so the wire-pure core of the crate
//! stays usable from wasm targets (this module is compiled out there).

use std::path::PathBuf;
use std::process::Child;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// `$XDG_RUNTIME_DIR`, or the fallback every caller re-derived by hand: the
/// uid's systemd user-runtime directory. Matches how systemd populates the
/// variable for user sessions, so the fallback only ever fires outside one.
pub fn xdg_runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", nix::unistd::getuid().as_raw())))
}

/// Open a pidfd for `pid` (Linux 5.3+).
///
/// Returns `Err` on old kernels or when the pid does not exist.
pub fn open_pidfd(pid: i32) -> std::io::Result<std::os::fd::OwnedFd> {
    let pid = rustix::process::Pid::from_raw(pid)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid PID"))?;
    rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty())
        .map_err(std::io::Error::from)
}

/// Register SIGTERM/SIGINT handlers that set the returned flag.
///
/// Callers poll this on their loop tick, so no SA_RESTART/EINTR coordination
/// is needed and every event loop turns the same flag into a clean exit.
pub fn install_shutdown_signal_handler() -> Arc<AtomicBool> {
    use signal_hook::consts::{SIGINT, SIGTERM};

    let shutdown = Arc::new(AtomicBool::new(false));
    for sig in [SIGTERM, SIGINT] {
        // Registration failure only happens before the flag exists; nothing
        // to return can express it better than ignoring a failed signal
        // handler like every duplicated copy did.
        let _ = signal_hook::flag::register(sig, Arc::clone(&shutdown));
    }
    shutdown
}

/// Wait for `child`, SIGKILLing it once `timeout` has elapsed.
pub fn wait_child_timeout(
    child: Child,
    timeout: Duration,
) -> std::io::Result<std::process::Output> {
    let pid = nix::unistd::Pid::from_raw(child.id().cast_signed());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if rx.recv_timeout(timeout).is_err() {
            let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
        }
    });
    let output = child.wait_with_output()?;
    let _ = tx.send(());
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Single-threaded test: set_var/remove_var are process-global and unsafe
    // in edition 2024, so this is wrapped in a targeted allowance.
    #[test]
    fn runtime_dir_prefers_the_env_and_falls_back_per_uid() {
        let saved = std::env::var_os("XDG_RUNTIME_DIR");
        // SAFETY: this test binary runs test bodies one at a time and no
        // other thread is spawned, so the global environment has no peer
        // reader while it is temporarily rewritten.
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", "/run/user/4242");
        }
        assert_eq!(xdg_runtime_dir(), PathBuf::from("/run/user/4242"));
        // SAFETY: same single-threaded contract as the set above.
        #[allow(unsafe_code)]
        unsafe {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        let fallback = xdg_runtime_dir();
        // SAFETY: restoring the original value for any later test.
        if let Some(v) = saved {
            // SAFETY: restoration of a prior env var under single-threaded
            // test conditions; matches the contract above.
            #[allow(unsafe_code)]
            unsafe {
                std::env::set_var("XDG_RUNTIME_DIR", v);
            }
        }
        assert!(
            fallback.starts_with("/run/user/") && !fallback.to_string_lossy().ends_with("4242"),
            "unexpected fallback {fallback:?}"
        );
    }
}
