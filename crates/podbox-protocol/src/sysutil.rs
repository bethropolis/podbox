//! Unix-only process helpers both the host CLI and the guest daemon need.
//!
//! Small, dependency-light implementations shared so each of them exists
//! exactly once: the built-in guest cannot be rebuilt for users, but its
//! behaviour tracks the host whenever the helpers agree. Keep this module
//! free of podbox types — it only touches the standard library plus the
//! `nix`/`rustix`/`signal-hook` shims, so the wire-pure core of the crate
//! stays usable from wasm targets (this module is compiled out there).

use std::io::{self, IoSlice};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
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

/// Send `data` on a Unix stream with file descriptors attached, so the
/// receiver reads both in one message.
///
/// This is the guest→host direction of descriptor passing (the host's
/// `process::recv_bytes_with_fds` mirrors it). Descriptors and frame bytes
/// travel together: when a `HostExecV2` request arrives, the host has the
/// stdio descriptors before it parses the frame. Retries on `EINTR`.
pub fn send_bytes_with_fds(stream: &UnixStream, data: &[&[u8]], fds: &[RawFd]) -> io::Result<()> {
    use nix::sys::socket::{ControlMessage, MsgFlags, sendmsg};

    let raw_fd = stream.as_raw_fd();
    let cmsg = ControlMessage::ScmRights(fds);
    let iov: Vec<IoSlice<'_>> = data.iter().map(|d| IoSlice::new(d)).collect();
    loop {
        match sendmsg::<()>(raw_fd, &iov, &[cmsg], MsgFlags::empty(), None) {
            Ok(_) => return Ok(()),
            Err(nix::errno::Errno::EINTR) => {}
            Err(e) => return Err(io::Error::from(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::IoSliceMut;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::os::fd::OwnedFd;

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

    /// Data and descriptors travel in one message: the frame payload is
    /// readable on the other end and the descriptor points at the file the
    /// sender chose.
    #[test]
    fn send_bytes_with_fds_delivers_data_and_descriptors_together() {
        let (tx, rx) = UnixStream::pair().unwrap();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"sent-by-test").unwrap();
        let file = std::fs::File::open(tmp.path()).unwrap();

        let frame = b"proton frame bytes";
        send_bytes_with_fds(&tx, &[&frame[..], b""], &[file.as_raw_fd()])
            .expect("sendmsg with rights");

        let mut buf = [0u8; 64];
        let (n, owned) = {
            let mut iov = [IoSliceMut::new(&mut buf)];
            let mut cmsg = vec![0u8; 64];
            let msg = nix::sys::socket::recvmsg::<()>(
                rx.as_raw_fd(),
                &mut iov,
                Some(&mut cmsg),
                nix::sys::socket::MsgFlags::MSG_CMSG_CLOEXEC,
            )
            .unwrap();
            let n = msg.bytes;
            let mut owned: Vec<OwnedFd> = Vec::new();
            if let Ok(cmsgs) = msg.cmsgs() {
                for cmsg in cmsgs {
                    if let nix::sys::socket::ControlMessageOwned::ScmRights(fds) = cmsg {
                        owned.extend(fds.into_iter().map(|r| {
                            // SAFETY: SCM_RIGHTS delivery transferred exactly one
                            // reference per descriptor to this process.
                            #[allow(unsafe_code)]
                            unsafe {
                                OwnedFd::from_raw_fd(r)
                            }
                        }));
                    }
                }
            }
            (n, owned)
        };
        assert_eq!(&buf[..n], frame, "frame bytes did not arrive");
        assert_eq!(owned.len(), 1);
        // Reopening through the descriptor proves it is the temp file.
        let target = tmp.path().to_path_buf();
        // Reopening through the descriptor proves it is the temp file.
        let mut fd_file = std::fs::File::from(owned.into_iter().next().unwrap());
        let mut contents = String::new();
        std::io::Read::read_to_string(&mut fd_file, &mut contents).unwrap();
        assert_eq!(contents, "sent-by-test");
        drop(target);
    }
}
