use std::collections::HashSet;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, sigaction};
use nix::sys::socket::{getsockopt, sockopt};

use crate::config::Config;
use crate::config::validation::parse_idle_timeout_secs;
use crate::process;
use crate::protocol::{GuestMessage, HostMessage, read_frame, write_frame};
use crate::systemd;

mod handlers;

/// Max number of concurrent host threads handling guest connections.
const MAX_CONCURRENT: usize = 4;

/// Max number of tracked terminal sessions (pidfd monitors).
const MAX_SESSIONS: u32 = 64;

/// How often the host sends a keepalive `Ping` to a connected guest.
const PING_INTERVAL: Duration = Duration::from_secs(60);

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Register SIGTERM/SIGINT handlers that set `SHUTDOWN_REQUESTED`.
/// Without SA_RESTART, blocking syscalls return EINTR, letting the
/// accept loop check the flag.
fn setup_signal_handler() -> nix::Result<()> {
    extern "C" fn handle_signal(_: i32) {
        SHUTDOWN_REQUESTED.store(true, Ordering::Relaxed);
    }
    let sig_action = SigAction::new(
        SigHandler::Handler(handle_signal),
        SaFlags::empty(),
        SigSet::empty(),
    );
    unsafe {
        sigaction(Signal::SIGTERM, &sig_action)?;
        sigaction(Signal::SIGINT, &sig_action)?;
    }
    Ok(())
}

/// Shared mutable state between all connections and PID monitor threads.
struct SharedState {
    /// Number of active terminal sessions tracked via pidfd.
    session_count: AtomicU32,
    /// Container name, for `systemctl stop` on idle timeout.
    container_name: String,
    /// Idle timeout in seconds (0 = disabled).
    idle_timeout_secs: u64,
    /// Whether this process was launched via systemd socket activation
    /// (`LISTEN_PID`/`LISTEN_FDS` set). If true, the process may
    /// self-terminate on idle timeout — systemd will re-spawn it via
    /// socket activation on the next connection.
    was_socket_activated: bool,
}

/// Run the host socket server for a container.
pub fn run(socket_path: &Path, config: &Config, container_name: &str) -> anyhow::Result<()> {
    let _ = setup_signal_handler();

    let config = config.clone();
    let path = socket_path.to_path_buf();
    let idle_timeout_secs = parse_idle_timeout_secs(&config.lifecycle.idle_timeout);

    let activation_fd = listen_fd();
    let was_socket_activated = activation_fd.is_some();
    let listener = match activation_fd {
        Some(fd) => unsafe { UnixListener::from_raw_fd(fd) },
        None => {
            let _ = std::fs::remove_file(&path);
            UnixListener::bind(&path)?
        }
    };

    let state = Arc::new(SharedState {
        session_count: AtomicU32::new(0),
        container_name: container_name.to_string(),
        idle_timeout_secs,
        was_socket_activated,
    });

    let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

    loop {
        if SHUTDOWN_REQUESTED.load(Ordering::Relaxed) {
            tracing::info!("podbox: shutdown requested, draining connections...");
            drop(listener);
            for h in handles {
                let _ = h.join();
            }
            return Ok(());
        }

        match listener.accept() {
            Ok((mut stream, _)) => {
                handles.retain_mut(|h| !h.is_finished());

                if handles.len() >= MAX_CONCURRENT {
                    tracing::warn!(
                        "dropping connection: {} concurrent clients already in flight",
                        handles.len()
                    );
                    continue;
                }

                let cfg = config.clone();
                let state = Arc::clone(&state);
                let handle = std::thread::spawn(move || {
                    if let Err(e) = handle_connection(&mut stream, &cfg, &state) {
                        tracing::error!("error handling connection: {}", e);
                    }
                });
                handles.push(handle);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => {
                tracing::error!("socket accept failed: {}", e);
                break;
            }
        }
    }

    Ok(())
}

fn listen_fd() -> Option<RawFd> {
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

fn handle_connection(
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
                let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
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

/// Maximum consecutive failed negotiations (bad hello, unauthenticated
/// privileged message, malformed frame) before the connection is dropped.
const MAX_NEGOTIATION_FAILURES: u32 = 5;

/// Count a failed negotiation and reply with a typed `Error` frame. Returns
/// true once the connection should be dropped (fail-closed after
/// `MAX_NEGOTIATION_FAILURES` failures).
fn note_failure(stream: &mut UnixStream, failures: &mut u32, reason: &str) -> bool {
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
fn has_cap(negotiated: &Option<HashSet<String>>, cap: &str) -> bool {
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

/// Block until `fd` (a pidfd) becomes readable, then decrement the session
/// counter.
fn monitor_pidfd(fd: OwnedFd, state: Arc<SharedState>) {
    let mut pfd = nix::libc::pollfd {
        fd: fd.as_raw_fd(),
        events: nix::libc::POLLIN,
        revents: 0,
    };

    loop {
        let ret = unsafe { nix::libc::poll(&raw mut pfd, 1, -1) };
        if ret < 0 {
            let errno = unsafe { *nix::libc::__errno_location() };
            if errno == nix::libc::EINTR {
                continue;
            }
            break;
        }
        if pfd.revents & (nix::libc::POLLIN | nix::libc::POLLHUP | nix::libc::POLLERR) != 0 {
            break;
        }
    }

    let _ = state.session_count.fetch_sub(1, Ordering::SeqCst);
    // Idle shutdown is driven entirely by the guest's own idle timer, so
    // there is no host-side work to do here.
}

#[cfg(test)]
mod tests {
    use super::handlers::{validate_host_exec_args, validate_uri};
    use super::{has_cap, note_failure};
    use std::collections::HashSet;

    // ── validate_uri tests ──

    #[test]
    fn allows_http_https_mailto() {
        assert_eq!(
            validate_uri("https://example.com"),
            Some("https://example.com".to_string())
        );
        assert_eq!(
            validate_uri("http://example.com"),
            Some("http://example.com".to_string())
        );
        assert_eq!(
            validate_uri("mailto:user@host"),
            Some("mailto:user@host".to_string())
        );
    }

    #[test]
    fn refuses_path_traversal() {
        assert_eq!(validate_uri("/etc/passwd"), None);
        assert_eq!(validate_uri("../foo"), None);
        assert_eq!(validate_uri(""), None);
    }

    #[test]
    fn refuses_unknown_alphabetic_schemes() {
        assert_eq!(validate_uri("javascript:alert(1)"), None);
        assert_eq!(validate_uri("file:///etc/passwd"), None);
    }

    #[test]
    fn wraps_bare_domain() {
        assert_eq!(
            validate_uri("example.com"),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(
            validate_uri("  https://example.com  "),
            Some("https://example.com".to_string())
        );
    }

    // ── validate_host_exec_args tests ──

    #[test]
    fn accepts_plain_args() {
        assert!(validate_host_exec_args(&["ls".into()]).is_ok());
        assert!(validate_host_exec_args(&["ls".into(), "-la".into(), "/tmp".into()]).is_ok());
        assert!(validate_host_exec_args(&["git".into(), "log".into(), "--oneline".into()]).is_ok());
    }

    #[test]
    fn rejects_shell_metacharacters() {
        assert!(validate_host_exec_args(&["echo".into(), "foo;bar".into()]).is_err());
        assert!(validate_host_exec_args(&["echo".into(), "foo|bar".into()]).is_err());
        assert!(validate_host_exec_args(&["echo".into(), "foo&bar".into()]).is_err());
        assert!(validate_host_exec_args(&["echo".into(), "$PATH".into()]).is_err());
        assert!(validate_host_exec_args(&["echo".into(), "`ls`".into()]).is_err());
    }

    #[test]
    fn rejects_redirection_operators() {
        assert!(validate_host_exec_args(&["cat".into(), "<file".into()]).is_err());
        assert!(validate_host_exec_args(&["echo".into(), ">file".into()]).is_err());
        assert!(validate_host_exec_args(&["echo".into(), ">>file".into()]).is_err());
    }

    #[test]
    fn rejects_glob_and_brace_chars() {
        assert!(validate_host_exec_args(&["ls".into(), "*.rs".into()]).is_err());
        assert!(validate_host_exec_args(&["ls".into(), "file?".into()]).is_err());
        assert!(validate_host_exec_args(&["ls".into(), "[abc]".into()]).is_err());
        assert!(validate_host_exec_args(&["echo".into(), "{a,b}".into()]).is_err());
    }

    #[test]
    fn rejects_subshell_and_escape_chars() {
        assert!(validate_host_exec_args(&["echo".into(), "$(whoami)".into()]).is_err());
        assert!(validate_host_exec_args(&["echo".into(), "line1\nline2".into()]).is_err());
    }

    #[test]
    fn rejects_restricted_flag_patterns() {
        assert!(validate_host_exec_args(&["git".into(), "--exec-path=/tmp".into()]).is_err());
        assert!(validate_host_exec_args(&["git".into(), "--config=user.name".into()]).is_err());
        assert!(validate_host_exec_args(&["vim".into(), "--plugin=malicious".into()]).is_err());
        assert!(validate_host_exec_args(&["python".into(), "--load=malicious".into()]).is_err());
        assert!(validate_host_exec_args(&["python".into(), "--module=malicious".into()]).is_err());
        assert!(validate_host_exec_args(&["git".into(), "--remote=evil".into()]).is_err());
        assert!(
            validate_host_exec_args(&[
                "ssh".into(),
                "-o".into(),
                "StrictHostKeyChecking=no".into()
            ])
            .is_err()
        );
    }

    #[test]
    fn restricted_flag_detection_is_case_insensitive() {
        assert!(validate_host_exec_args(&["git".into(), "--EXEC-PATH=/tmp".into()]).is_err());
        assert!(validate_host_exec_args(&["GIT".into(), "--Config=evil".into()]).is_err());
    }

    #[test]
    fn does_not_restrict_safe_flags() {
        assert!(validate_host_exec_args(&["git".into(), "--exec".into()]).is_ok());
        assert!(
            validate_host_exec_args(&["git".into(), "--exec-path-is-ok".into()]).is_err(),
            "--exec-path prefix still blocked"
        );
        assert!(validate_host_exec_args(&["ls".into(), "--color=auto".into()]).is_ok());
        assert!(validate_host_exec_args(&["cargo".into(), "--offline".into()]).is_ok());
    }

    #[test]
    fn rejects_empty_args_gracefully() {
        assert!(
            validate_host_exec_args(&[String::new()]).is_ok(),
            "empty string is not a metachar"
        );
    }

    #[test]
    fn ascii_lowercase_only() {
        assert!(validate_host_exec_args(&["git".into(), "--EXEC-PATH=".into()]).is_err());
        assert!(
            validate_host_exec_args(&["git".into(), "--\u{0130}".into()]).is_ok(),
            "Turkish \u{0130} is non-ASCII"
        );
    }

    // ── has_cap tests ──

    #[test]
    fn has_cap_none_negotiated_rejects() {
        assert!(!has_cap(&None, crate::protocol::CAP_NOTIFY));
        assert!(!has_cap(&None, crate::protocol::CAP_CLIPBOARD));
    }

    #[test]
    fn has_cap_accepts_negotiated() {
        let caps = HashSet::from(["notify".to_string(), "clipboard".to_string()]);
        let negotiated = Some(caps);
        assert!(has_cap(&negotiated, crate::protocol::CAP_NOTIFY));
        assert!(has_cap(&negotiated, crate::protocol::CAP_CLIPBOARD));
        assert!(!has_cap(&negotiated, crate::protocol::CAP_XDG_OPEN));
        assert!(!has_cap(&negotiated, crate::protocol::CAP_HOST_EXEC));
    }

    // ── note_failure tests ──

    fn socket_pair() -> (
        std::os::unix::net::UnixStream,
        std::os::unix::net::UnixStream,
    ) {
        std::os::unix::net::UnixStream::pair().expect("socketpair")
    }

    #[test]
    fn note_failure_is_false_below_threshold() {
        let (mut server, _client) = socket_pair();
        let mut failures: u32 = 0;
        for _ in 0..(super::MAX_NEGOTIATION_FAILURES - 1) {
            assert!(!note_failure(&mut server, &mut failures, "probe"));
        }
        assert_eq!(failures, super::MAX_NEGOTIATION_FAILURES - 1);
    }

    #[test]
    fn note_failure_drops_after_threshold() {
        let (mut server, mut client) = socket_pair();
        let mut failures: u32 = 0;
        for _ in 0..(super::MAX_NEGOTIATION_FAILURES - 1) {
            assert!(!note_failure(&mut server, &mut failures, "probe"));
        }
        assert!(note_failure(&mut server, &mut failures, "probe"));
        assert_eq!(failures, super::MAX_NEGOTIATION_FAILURES);

        // Every failure was answered with a typed Error frame.
        use crate::protocol::read_frame;
        for _ in 0..super::MAX_NEGOTIATION_FAILURES {
            let bytes = read_frame(&mut client).unwrap().expect("error frame");
            let msg: crate::protocol::HostMessage = serde_json::from_slice(&bytes).unwrap();
            assert!(
                matches!(msg, crate::protocol::HostMessage::Error { reason } if reason == "probe")
            );
        }
    }

    // ── handle_hello tests ──

    #[test]
    fn hello_protocol_mismatch_is_rejected() {
        use super::handlers::{HelloOutcome, handle_hello};
        let (mut server, mut client) = socket_pair();
        let config = crate::config::Config::embedded();
        let outcome = handle_hello(
            &mut server,
            &config.integration,
            0,
            crate::protocol::PROTOCOL_VERSION + 1,
            "test".into(),
            "test".into(),
            vec![],
        )
        .unwrap();
        assert!(matches!(outcome, HelloOutcome::Rejected));

        // The peer is told to shut down, and no capabilities are granted.
        let bytes = crate::protocol::read_frame(&mut client)
            .unwrap()
            .expect("shutdown frame");
        let msg: crate::protocol::HostMessage = serde_json::from_slice(&bytes).unwrap();
        assert!(matches!(msg, crate::protocol::HostMessage::Shutdown));
    }

    #[test]
    fn hello_accepts_enabled_capabilities() {
        use super::handlers::{HelloOutcome, handle_hello};
        let (mut server, mut client) = socket_pair();
        let mut config = crate::config::Config::embedded();
        config.integration.notify = true;
        config.integration.clipboard = true;
        config.integration.xdg_open = false;
        let outcome = handle_hello(
            &mut server,
            &config.integration,
            0,
            crate::protocol::PROTOCOL_VERSION,
            "test".into(),
            "test".into(),
            vec![
                crate::protocol::CAP_NOTIFY.to_string(),
                crate::protocol::CAP_XDG_OPEN.to_string(),
            ],
        )
        .unwrap();
        let HelloOutcome::Accepted(accepted) = outcome else {
            panic!("expected Accepted");
        };
        assert_eq!(accepted, vec![crate::protocol::CAP_NOTIFY]);

        let bytes = crate::protocol::read_frame(&mut client)
            .unwrap()
            .expect("hello ack");
        let msg: crate::protocol::HostMessage = serde_json::from_slice(&bytes).unwrap();
        match msg {
            crate::protocol::HostMessage::HelloAck {
                accepted, rejected, ..
            } => {
                assert_eq!(accepted, vec![crate::protocol::CAP_NOTIFY]);
                assert_eq!(rejected, vec![crate::protocol::CAP_XDG_OPEN]);
            }
            other => panic!("expected HelloAck, got {other:?}"),
        }
    }
}
