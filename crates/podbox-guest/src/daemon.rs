//! Guest-side daemon lifecycle: startup, reconnect, and the poll event loop.

use std::collections::HashSet;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

use crate::error::GuestError;
use crate::protocol::{GuestMessage, HostMessage, write_frame};
use crate::socket;

mod idle;
mod services;
mod setup;
mod user_path;

use idle::{
    IDLE_CONFIRM_MS, TrackedProcess, has_event, idle_poll_ms, scan_user_processes, track_processes,
};
use setup::{check_version_drift, install_interceptors, write_path_injection};
use user_path::resolve_user_path;

/// Register SIGTERM/SIGINT handlers via the shared sysutil flag.
///
/// The event loop polls the flag (and tolerates `EINTR`), so no
/// `SA_RESTART` coordination is needed.
fn setup_signal_handler() -> Arc<AtomicBool> {
    podbox_protocol::sysutil::install_shutdown_signal_handler()
}

pub fn run() -> Result<(), GuestError> {
    services::start_service_supervisor();
    let shutdown = setup_signal_handler();

    let host_socket_path = socket::host_socket_path()?;
    let container_name = socket::container_name()?;
    let bin_dir = PathBuf::from("/run/podbox/bin");

    // 1. Create /run/podbox/bin/ and expose guest version for fastfetch/host
    std::fs::create_dir_all(&bin_dir)?;
    let _ = std::fs::write("/run/podbox/guest-version", crate::VERSION);

    // 2. Connect to host socket with retry
    tracing::info!("guest: connecting to host socket...");
    let mut host_stream = socket::connect_to_host(&host_socket_path)?;

    // 3. Handshake
    let all_caps: Vec<String> = crate::protocol::ALL_CAPABILITIES
        .iter()
        .map(|&s| s.to_string())
        .collect();
    let (accepted, mut idle_timeout_secs, host_exec_shims, host_version) =
        socket::handshake(&mut host_stream, &container_name, &all_caps)?;
    let accepted_set: HashSet<String> = accepted.iter().cloned().collect();
    tracing::info!("guest: accepted capabilities: {accepted:?}");

    // 4. Check version drift
    check_version_drift(
        &accepted_set,
        &mut host_stream,
        &container_name,
        &host_version,
    );

    // 5. Install interceptor symlinks for accepted capabilities
    install_interceptors(&accepted_set, &bin_dir, &host_exec_shims)?;

    // 6. Write PATH injection
    write_path_injection(&bin_dir)?;

    // 7. Resolve and export the user's full PATH for host-side consumption
    resolve_user_path();

    // 8. Handle connections and self-heal on host disconnects/restarts.
    // Only a dropped connection reconnects: a shutdown (signal or host
    // message) exits the process, and a version mismatch is fatal.
    let mut backoff_failures = 0u32;
    loop {
        match event_loop(&mut host_stream, idle_timeout_secs, &shutdown) {
            Exit::Shutdown => {
                tracing::info!("guest: shutdown requested, exiting.");
                return Ok(());
            }
            Exit::Disconnected => {
                if shutdown.load(Ordering::Relaxed) {
                    return Ok(());
                }
                tracing::warn!("guest: host disconnected. Retrying connection...");
                // Retry connect + handshake in an inner loop: a failed
                // handshake must not fall through to `event_loop` on a stream
                // that was never negotiated.
                loop {
                    backoff_failures = backoff_failures.saturating_add(1);
                    std::thread::sleep(backoff_delay(backoff_failures));
                    if shutdown.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    let stream = match socket::connect_to_host(&host_socket_path) {
                        Ok(stream) => stream,
                        Err(e) => {
                            tracing::error!("guest: reconnect failed: {e}");
                            continue;
                        }
                    };
                    host_stream = stream;
                    match socket::handshake(&mut host_stream, &container_name, &all_caps) {
                        Ok((caps, idle, shims, _host_version)) => {
                            tracing::info!(
                                "guest: re-established connection and handshook successfully."
                            );
                            backoff_failures = 0;
                            idle_timeout_secs = idle;
                            let caps_set: HashSet<String> = caps.iter().cloned().collect();
                            let _ = install_interceptors(&caps_set, &bin_dir, &shims);
                            let _ = write_path_injection(&bin_dir);
                            break;
                        }
                        Err(GuestError::VersionMismatch(reason)) => {
                            tracing::error!("guest: {reason}; not reconnecting.");
                            return Err(GuestError::VersionMismatch(reason));
                        }
                        Err(e) => {
                            tracing::error!("guest: re-handshake failed: {e}");
                        }
                    }
                }
            }
        }
    }
}

/// How the daemon's connection to the host ended.
#[derive(Debug, PartialEq, Eq)]
enum Exit {
    /// SIGTERM/SIGINT arrived, or the host sent `Shutdown`: exit the
    /// process instead of reconnecting.
    Shutdown,
    /// The connection dropped (EOF, hangup, I/O error): reconnect.
    Disconnected,
}

/// Reconnect wait after `failures` consecutive failed sessions: 1 s
/// doubling up to a 60 s cap. Reset to zero after a successful session.
fn backoff_delay(failures: u32) -> std::time::Duration {
    let secs = 1u64 << failures.saturating_sub(1).min(6);
    std::time::Duration::from_secs(secs.min(60))
}

#[allow(clippy::too_many_lines)]
fn event_loop(host_stream: &mut UnixStream, idle_timeout_secs: u64, shutdown: &AtomicBool) -> Exit {
    let idle_limit_ms = idle_timeout_secs.saturating_mul(1000);
    let mut tracked: Vec<TrackedProcess> = Vec::new();
    // Deadline-based idle timer: compute remaining time from a fixed start
    // instant so a host Ping waking poll() can no longer restart the window.
    let mut idle_deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(idle_limit_ms);

    loop {
        if shutdown.load(Ordering::Relaxed) {
            tracing::info!("guest: received shutdown signal, exiting.");
            return Exit::Shutdown;
        }

        let host_revents: PollFlags;
        let pid_revents: Vec<PollFlags>;

        {
            let mut fds: Vec<PollFd> = Vec::with_capacity(1 + tracked.len());
            fds.push(PollFd::new(host_stream.as_fd(), PollFlags::POLLIN));
            for proc in &tracked {
                fds.push(PollFd::new(proc.fd.as_fd(), PollFlags::POLLIN));
            }

            let timeout = match idle_poll_ms(
                idle_timeout_secs > 0,
                tracked.is_empty(),
                idle_deadline,
                std::time::Instant::now(),
            ) {
                Some(ms) => PollTimeout::from(Some(ms)),
                None => PollTimeout::from(None::<u16>),
            };

            match poll(&mut fds, timeout) {
                Ok(0) => {
                    if tracked.is_empty() {
                        let mut active = scan_user_processes();
                        if active.is_empty() {
                            // Confirm we're still idle after a short grace
                            // before killing the box, so a task started in
                            // the window since the last scan is not lost.
                            std::thread::sleep(std::time::Duration::from_millis(IDLE_CONFIRM_MS));
                            active = scan_user_processes();
                        }
                        if active.is_empty() {
                            let _ = write_frame(host_stream, &GuestMessage::IdleTimeout);
                            // Our own idle timer fired: the host will
                            // stop the unit. Exit instead of
                            // reconnecting to a container going down.
                            return Exit::Shutdown;
                        }
                        tracked = track_processes(&active);
                        idle_deadline = std::time::Instant::now()
                            + std::time::Duration::from_millis(idle_limit_ms);
                    }
                    continue;
                }
                Ok(_) => {
                    host_revents = fds[0].revents().unwrap_or(PollFlags::empty());
                    pid_revents = fds[1..]
                        .iter()
                        .map(|f| f.revents().unwrap_or(PollFlags::empty()))
                        .collect();
                }
                Err(nix::errno::Errno::EINTR) => {
                    if shutdown.load(Ordering::Relaxed) {
                        tracing::info!("guest: received shutdown signal, exiting.");
                        return Exit::Shutdown;
                    }
                    continue;
                }
                Err(_) => return Exit::Disconnected,
            }
        }

        // ── Host socket events ──
        // POLLIN first: a peer that writes and then closes delivers its
        // last message alongside POLLHUP. That message (e.g. `Shutdown`)
        // must be processed, not discarded as a hangup.
        if host_revents.contains(PollFlags::POLLIN) {
            match socket::read_host_message(host_stream) {
                Ok(Some(HostMessage::Shutdown)) => {
                    tracing::info!("guest: received shutdown, exiting.");
                    return Exit::Shutdown;
                }
                Ok(Some(
                    HostMessage::Ping
                    | HostMessage::HelloAck { .. }
                    | HostMessage::ClipboardData { .. }
                    | HostMessage::HostExecStdout { .. }
                    | HostMessage::HostExecStderr { .. }
                    | HostMessage::HostExecOutput { .. }
                    | HostMessage::HostExecDone { .. }
                    | HostMessage::NotifyActionResult { .. }
                    | HostMessage::Error { .. },
                )) => {}
                Ok(Some(HostMessage::GetInfo)) => {
                    let _ = write_frame(
                        host_stream,
                        &GuestMessage::Info {
                            guest_version: crate::VERSION.to_string(),
                            protocol_version: crate::protocol::PROTOCOL_VERSION,
                        },
                    );
                }
                Ok(Some(HostMessage::CheckIdle)) => {
                    let mut active = scan_user_processes();
                    if active.is_empty() {
                        std::thread::sleep(std::time::Duration::from_millis(IDLE_CONFIRM_MS));
                        active = scan_user_processes();
                    }
                    if active.is_empty() {
                        let _ = write_frame(host_stream, &GuestMessage::IdleTimeout);
                    } else {
                        tracked = track_processes(&active);
                        idle_deadline = std::time::Instant::now()
                            + std::time::Duration::from_millis(idle_limit_ms);
                        let _ = write_frame(host_stream, &GuestMessage::Busy);
                    }
                }
                Ok(None) => {
                    tracing::warn!("guest: host disconnected.");
                    return Exit::Disconnected;
                }
                Err(GuestError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // Spurious poll wakeup: POLLIN was reported but there is
                    // nothing to read yet. Fall through to the hangup check.
                }
                Err(_) => return Exit::Disconnected,
            }
        }

        if host_revents.contains(PollFlags::POLLHUP) || host_revents.contains(PollFlags::POLLERR) {
            tracing::warn!("guest: host socket hung up.");
            return Exit::Disconnected;
        }

        // ── pidfd events (tracked process exits) ──
        let mut exited: Vec<usize> = Vec::new();
        for (i, rev) in pid_revents.iter().enumerate() {
            if has_event(*rev) {
                exited.push(i);
            }
        }

        for &i in exited.iter().rev() {
            tracked.remove(i);
        }

        if !exited.is_empty() && tracked.is_empty() {
            let active = scan_user_processes();
            tracked = track_processes(&active);
            // Restart the idle window when the last tracked process exits,
            // even if the scan found nothing: the previous deadline may be in
            // the past, which would otherwise shut the box down immediately
            // instead of after a fresh timeout.
            idle_deadline =
                std::time::Instant::now() + std::time::Duration::from_millis(idle_limit_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── shutdown / reconnect behavior ──

    /// Drive `event_loop` on one end of a socket pair. The result comes
    /// back over a channel with a deadline, so a loop that never returns
    /// fails the test instead of hanging the suite.
    fn run_event_loop_once(
        server: std::os::unix::net::UnixStream,
        idle_timeout_secs: u64,
        preset_shutdown: bool,
    ) -> Exit {
        let shutdown = AtomicBool::new(preset_shutdown);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut stream = server;
            let _ = tx.send(event_loop(&mut stream, idle_timeout_secs, &shutdown));
        });
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("event loop did not return")
    }

    #[test]
    fn backoff_delay_doubles_up_to_a_cap() {
        let secs: Vec<u64> = (0..=8).map(|f| backoff_delay(f).as_secs()).collect();
        assert_eq!(secs, vec![1, 1, 2, 4, 8, 16, 32, 60, 60]);
        assert_eq!(backoff_delay(u32::MAX).as_secs(), 60);
    }

    #[test]
    fn shutdown_message_makes_the_event_loop_exit() {
        let (server, mut mock) = std::os::unix::net::UnixStream::pair().unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = write_frame(&mut mock, &HostMessage::Shutdown);
        });
        assert_eq!(run_event_loop_once(server, 0, false), Exit::Shutdown);
    }

    #[test]
    fn closed_host_connection_is_a_disconnect_not_a_shutdown() {
        let (server, mock) = std::os::unix::net::UnixStream::pair().unwrap();
        drop(mock);
        assert_eq!(run_event_loop_once(server, 0, false), Exit::Disconnected);
    }

    #[test]
    fn preset_shutdown_flag_exits_without_reading() {
        let (server, _mock) = std::os::unix::net::UnixStream::pair().unwrap();
        assert_eq!(run_event_loop_once(server, 0, true), Exit::Shutdown);
    }

    #[test]
    fn idle_timeout_zero_disables_the_timer() {
        let now = std::time::Instant::now();
        assert_eq!(idle_poll_ms(false, true, now, now), None);
    }

    #[test]
    fn idle_poll_arms_only_without_tracked_processes() {
        let now = std::time::Instant::now();
        let deadline = now + std::time::Duration::from_secs(30);
        assert_eq!(idle_poll_ms(true, false, deadline, now), None);
        assert_eq!(idle_poll_ms(true, true, now, now), Some(0));
        assert_eq!(idle_poll_ms(true, true, deadline, now), Some(30_000));
    }
}
