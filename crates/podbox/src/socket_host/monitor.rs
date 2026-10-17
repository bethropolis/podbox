//! Signal handling, systemd socket-activation fd adoption, and the pidfd
//! session monitor for the host socket server.
//!
//! Extracted verbatim from `socket_host.rs`.

use std::os::fd::{AsFd, OwnedFd, RawFd};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::SharedState;

pub(crate) fn listen_fd() -> Option<RawFd> {
    let pid = std::env::var("LISTEN_PID").ok()?.parse::<u32>().ok()?;
    if pid != std::process::id() {
        return None;
    }
    let fds = std::env::var("LISTEN_FDS").ok()?.parse::<u32>().ok()?;
    if fds == 0 {
        return None;
    }
    Some(3)
}

/// Block until `fd` (a pidfd) becomes readable, then decrement the session
/// counter.
pub(crate) fn monitor_pidfd(fd: OwnedFd, state: Arc<SharedState>) {
    let mut fds = [nix::poll::PollFd::new(
        fd.as_fd(),
        nix::poll::PollFlags::POLLIN,
    )];

    loop {
        match nix::poll::poll(&mut fds, nix::poll::PollTimeout::NONE) {
            Ok(_) => break,
            Err(nix::errno::Errno::EINTR) => {}
            Err(_) => break,
        }
    }

    let _ = state.session_count.fetch_sub(1, Ordering::SeqCst);
    // Idle shutdown is driven entirely by the guest's own idle timer, so
    // there is no host-side work to do here.
}
