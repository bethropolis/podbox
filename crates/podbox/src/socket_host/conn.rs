//! Per-connection message loop for the host socket server: capability
//! negotiation (`Hello`), message dispatch to [`super::handlers`], and
//! negotiation-failure accounting.
//!
//! Extracted verbatim from `socket_host.rs`.

use std::collections::HashSet;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use nix::sys::socket::{getsockopt, sockopt};

use crate::config::Config;
use crate::process;
use crate::protocol::{GuestMessage, HostMessage, read_frame, write_frame};
use crate::systemd;

use super::handlers;
use super::monitor::monitor_pidfd;
use super::{MAX_NEGOTIATION_FAILURES, MAX_SESSIONS, PING_INTERVAL, SharedState};

pub(crate) fn handle_connection(
    stream: &mut UnixStream,
    config: &Config,
    state: &Arc<SharedState>,
) -> anyhow::Result<()> {
    stream.set_read_timeout(Some(PING_INTERVAL))?;
    let mut last_ping = std::time::Instant::now();
    // Capabilities accepted for this connection during `Hello`. Privileged
    // messages are rejected until this is populated, and each is further
    // gated on the specific capability the admin enabled.
    let mut negotiated: Option<HashSet<String>> = None;
    // Consecutive failed negotiations. The connection is dropped (fail-closed)
    // once this reaches `MAX_NEGOTIATION_FAILURES`.
    let mut failures: u32 = 0;

    loop {
        let msg_bytes = match read_frame(stream) {
            Ok(Some(b)) => b,
            Ok(None) => return Ok(()),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                if last_ping.elapsed() >= PING_INTERVAL {
                    if write_frame(stream, &HostMessage::Ping).is_err() {
                        return Ok(());
                    }
                    last_ping = std::time::Instant::now();
                }
                continue;
            }
            Err(e) => return Err(e.into()),
        };

        last_ping = std::time::Instant::now();
        let msg: GuestMessage = match serde_json::from_slice(&msg_bytes) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("malformed frame from peer: {e}");
                if note_failure(stream, &mut failures, "malformed frame") {
                    return Ok(());
                }
                continue;
            }
        };

        match msg {
            GuestMessage::Hello {
                protocol_version,
                guest_version,
                container,
                capabilities,
            } => {
                let outcome = handlers::handle_hello(
                    stream,
                    &config.integration,
                    state.idle_timeout_secs,
                    protocol_version,
                    guest_version,
                    container,
                    capabilities,
                )?;
                let handlers::HelloOutcome::Accepted(accepted) = outcome else {
                    // Failed negotiation: no capabilities granted, daemon stream
                    // stays unclaimed. Drop after repeated failures.
                    if note_failure(stream, &mut failures, "hello rejected") {
                        return Ok(());
                    }
                    continue;
                };
                negotiated = Some(accepted.into_iter().collect());

                // Idle shutdown is driven entirely by the guest's own idle
                // timer (which respects idle_timeout). The host must NOT
                // probe the daemon immediately on hello: a container that was
                // just started is, by definition, not yet idle, and stopping
                // it at hello races `podman enter`, which is trying to spawn
                // a session into it. An immediate check would kill the box
                // before the user's shell can start.
            }
            GuestMessage::RegisterSession => {
                // host-CLI-only: the peer must be inside the host user
                // namespace (i.e. running on the host, not in the container).
                if !peer_is_in_host_userns(stream) {
                    tracing::warn!("rejecting RegisterSession from foreign user namespace");
                    let _ = write_frame(
                        stream,
                        &HostMessage::Error {
                            reason: "register_session is host-only".into(),
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
                        },
                    );
                    return Ok(());
                }
                // Receive the pidfd via SCM_RIGHTS
                let raw_fd = match process::recv_fd(stream) {
                    Ok(Some(fd)) => fd,
                    Ok(None) => return Ok(()),
                    Err(_) => return Ok(()),
                };
                let fd = process::adopt_scm_fd(raw_fd);
                state.session_count.fetch_add(1, Ordering::SeqCst);
                let s = Arc::clone(state);
                std::thread::spawn(move || monitor_pidfd(fd, s));
                // Return immediately — the CLI closes the connection after
                // sending RegisterSession + pidfd.
                return Ok(());
            }
            GuestMessage::Busy => {
                if negotiated.is_none() {
                    if note_failure(stream, &mut failures, "hello required") {
                        return Ok(());
                    }
                }
            }
            GuestMessage::IdleTimeout => {
                if negotiated.is_none() {
                    if note_failure(stream, &mut failures, "hello required") {
                        return Ok(());
                    }
                    continue;
                }
                if state.idle_timeout_secs > 0 {
                    let name = &state.container_name;
                    tracing::info!("container '{}' idle — stopping", name);
                    let _ = systemd::stop_unit(name);
                    // If socket-activated, self-terminate so the host
                    // service doesn't sit resident forever.  systemd
                    // re-spawns it via socket activation on the next
                    // connection.  Non-systemd (manual bind) must stay
                    // alive — it has no re-launch mechanism.
                    if state.was_socket_activated {
                        std::process::exit(0);
                    }
                }
            }
            GuestMessage::Notify {
                summary,
                body,
                urgency: _,
                actions,
                app_name: _,
            } => {
                if !has_cap(&negotiated, crate::protocol::CAP_NOTIFY) {
                    if note_failure(stream, &mut failures, "capability 'notify' not accepted") {
                        return Ok(());
                    }
                    continue;
                }
                handlers::handle_notify(stream, summary, body, actions)?
            }
            GuestMessage::XdgOpen { uri } => {
                if !has_cap(&negotiated, crate::protocol::CAP_XDG_OPEN) {
                    if note_failure(stream, &mut failures, "capability 'xdg_open' not accepted") {
                        return Ok(());
                    }
                    continue;
                }
                handlers::handle_xdg_open(uri)?
            }
            GuestMessage::ClipboardSet { text } => {
                if !has_cap(&negotiated, crate::protocol::CAP_CLIPBOARD) {
                    if note_failure(stream, &mut failures, "capability 'clipboard' not accepted") {
                        return Ok(());
                    }
                    continue;
                }
                handlers::handle_clipboard_set(text)?
            }
            GuestMessage::ClipboardGet => {
                if !has_cap(&negotiated, crate::protocol::CAP_CLIPBOARD) {
                    if note_failure(stream, &mut failures, "capability 'clipboard' not accepted") {
                        return Ok(());
                    }
                    continue;
                }
                handlers::handle_clipboard_get(stream)?
            }
            GuestMessage::HostExec { cmd, args } => {
                if !has_cap(&negotiated, crate::protocol::CAP_HOST_EXEC) {
                    if note_failure(stream, &mut failures, "capability 'host_exec' not accepted") {
                        return Ok(());
                    }
                    continue;
                }
                handlers::handle_host_exec(stream, &config.integration, cmd, args)?
            }
        }
    }
}

/// Count a failed negotiation and reply with a typed `Error` frame. Returns
/// true once the connection should be dropped (fail-closed after
/// `MAX_NEGOTIATION_FAILURES` failures).
pub(crate) fn note_failure(stream: &mut UnixStream, failures: &mut u32, reason: &str) -> bool {
    *failures = failures.saturating_add(1);
    let _ = write_frame(
        stream,
        &HostMessage::Error {
            reason: reason.to_string(),
        },
    );
    *failures >= MAX_NEGOTIATION_FAILURES
}

/// True if `negotiated` contains the given capability.
pub(crate) fn has_cap(negotiated: &Option<HashSet<String>>, cap: &str) -> bool {
    negotiated
        .as_ref()
        .is_some_and(|caps| caps.iter().any(|c| c == cap))
}

/// Whether the peer of `stream` lives in the host user namespace.
///
/// Compares `SO_PEERCRED`'s pid against `/proc/self/ns/user`. The host CLI
/// runs in the host userns; anything inside the container runs in the
/// container's private userns (rootless podman), so the inode differs.
fn peer_is_in_host_userns(stream: &UnixStream) -> bool {
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
