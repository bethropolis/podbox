use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use nix::sys::socket::{getsockopt, sockopt};

use crate::process;
use crate::protocol::{HostMessage, write_frame};

use super::{MAX_SESSIONS, SharedState};

/// Register a host-side terminal session and start its pidfd monitor.
pub(crate) fn register_session(
    stream: &mut UnixStream,
    state: &Arc<SharedState>,
) -> anyhow::Result<()> {
    // host-CLI-only: the peer must be inside the host user namespace (i.e.
    // running on the host, not in the container).
    if !peer_is_in_host_userns(stream) {
        tracing::warn!("rejecting RegisterSession from foreign user namespace");
        let _ = write_frame(
            stream,
            &HostMessage::Error {
                reason: "register_session is host-only".into(),
                code: None,
            },
        );
        return Ok(());
    }
    if state.session_count.load(Ordering::SeqCst) >= MAX_SESSIONS {
        tracing::warn!("rejecting RegisterSession: session cap reached");
        let _ = write_frame(
            stream,
            &HostMessage::Error {
                reason: "session limit reached".into(),
                code: Some("busy".into()),
            },
        );
        return Ok(());
    }
    // Receive the pidfd via SCM_RIGHTS.
    let fd = match process::recv_fd(stream) {
        Ok(Some(fd)) => fd,
        Ok(None) | Err(_) => return Ok(()),
    };
    state.session_count.fetch_add(1, Ordering::SeqCst);
    let state = Arc::clone(state);
    std::thread::spawn(move || monitor_pidfd(fd, state));
    Ok(())
}

/// Whether the peer of `stream` lives in the host user namespace.
///
/// Compares `SO_PEERCRED`'s pid against `/proc/self/ns/user`. The host CLI
/// runs in the host userns; anything inside the container runs in the
/// container's private userns (rootless podman), so the inode differs.
///
/// Note the TOCTOU caveat: the pid is read from the socket, then
/// `/proc/<pid>/ns/user` is read in a separate step, so a recycled pid
/// could point at the wrong process. A pidfd-based check would close this.
pub(crate) fn peer_is_in_host_userns(stream: &UnixStream) -> bool {
    let creds = match getsockopt(stream, sockopt::PeerCredentials) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let self_ns = std::fs::read_link("/proc/self/ns/user").ok();
    let peer_ns = std::fs::read_link(format!("/proc/{}/ns/user", creds.pid())).ok();
    match (self_ns, peer_ns) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Block until `fd` (a pidfd) becomes readable, then decrement the session
/// counter.
fn monitor_pidfd(fd: OwnedFd, state: Arc<SharedState>) {
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
