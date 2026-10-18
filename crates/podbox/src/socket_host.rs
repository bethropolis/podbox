use std::os::fd::{AsFd, FromRawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use crate::config::Config;
use crate::config::validation::parse_idle_timeout_secs;
use crate::protocol::{HostMessage, write_frame};

mod actions;
mod conn;
mod handlers;
mod monitor;

pub use actions::{HostActions, SystemActions};

use conn::handle_connection;
use conn::peer_is_in_host_userns;
use monitor::listen_fd;

/// Maximum consecutive failed negotiations (bad hello, unauthenticated
/// privileged message, malformed frame) before the connection is dropped.
const MAX_NEGOTIATION_FAILURES: u32 = 5;

/// Max number of concurrent host threads handling guest connections.
const MAX_CONCURRENT: usize = 4;

/// Extra slots beyond [`MAX_CONCURRENT`] reserved for host-side peers: the
/// host CLI's `RegisterSession` and doctor probes, which all pass
/// `peer_is_in_host_userns`. A pool full of guest connections (silent
/// squatters are bounded by `HELLO_DEADLINE`, but a valid Hello holds a
/// slot for as long as its handler runs) must not starve host control
/// traffic.
const HOST_OVERFLOW: usize = 2;

/// Max number of tracked terminal sessions (pidfd monitors).
const MAX_SESSIONS: u32 = 64;

/// How often the host sends a keepalive `Ping` to a connected guest.
const PING_INTERVAL: Duration = Duration::from_mins(1);

/// A new connection must send `Hello` within this long, or it is closed. A
/// silent peer then holds a slot for seconds rather than indefinitely.
pub(crate) const HELLO_DEADLINE: Duration = Duration::from_secs(5);

/// Once a frame has started, the rest of it must arrive within this long.
pub(crate) const FRAME_DEADLINE: Duration = Duration::from_secs(10);

/// Shared mutable state between all connections and PID monitor threads.
pub(crate) struct SharedState {
    /// Number of active terminal sessions tracked via pidfd.
    pub(crate) session_count: AtomicU32,
    /// Container name, for `systemctl stop` on idle timeout.
    pub(crate) container_name: String,
    /// Idle timeout in seconds (0 = disabled).
    pub(crate) idle_timeout_secs: u64,
    /// Whether this process was launched via systemd socket activation
    /// (`LISTEN_PID`/`LISTEN_FDS` set). If true, the process may
    /// self-terminate on idle timeout — systemd will re-spawn it via
    /// socket activation on the next connection.
    pub(crate) was_socket_activated: bool,
}

/// Serve one guest connection with the real message loop and the given host
/// actions. [`run`] uses the same loop with [`SystemActions`]. This entry point
/// lets the protocol tests drive that loop over a socket pair with a fake.
pub fn serve_connection(
    stream: &mut UnixStream,
    config: &Config,
    container_name: &str,
    actions: &dyn HostActions,
) -> anyhow::Result<()> {
    let state = Arc::new(SharedState {
        session_count: AtomicU32::new(0),
        container_name: container_name.to_string(),
        idle_timeout_secs: parse_idle_timeout_secs(&config.lifecycle.idle_timeout),
        was_socket_activated: false,
    });
    handle_connection(stream, config, &state, actions)
}

/// Whether a new connection may be accepted with `active` connections
/// already in flight. Guest peers are capped at `max`; host-side peers
/// may additionally use `overflow` reserved slots.
fn connection_allowed(active: usize, max: usize, overflow: usize, peer_is_host: bool) -> bool {
    active < max || (peer_is_host && active < max + overflow)
}

/// The accept loop behind [`run`]: accept, enforce the connection cap with
/// reserved host slots, and hand each connection to a thread running the
/// real message loop.
///
/// `classify_host_peer` decides whether a fresh connection is host-side
/// (production: [`peer_is_in_host_userns`]); it is a parameter so tests can
/// script the peer mix without joining new user namespaces.
fn accept_loop(
    listener: UnixListener,
    config: &Config,
    state: &Arc<SharedState>,
    shutdown: &AtomicBool,
    classify_host_peer: &dyn Fn(&UnixStream) -> bool,
    max: usize,
    overflow: usize,
) -> anyhow::Result<()> {
    // Non-blocking + periodic tick so SIGTERM/SIGINT ends the accept loop
    // promptly instead of blocking in accept(2) until systemd's
    // TimeoutStopSec SIGKILL.
    listener.set_nonblocking(true)?;

    let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

    loop {
        if shutdown.load(Ordering::Relaxed) {
            tracing::info!("podbox: shutdown requested, draining connections...");
            drop(listener);
            for h in handles {
                let _ = h.join();
            }
            return Ok(());
        }

        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false)?;
                handles.retain_mut(|h| !h.is_finished());

                if !connection_allowed(handles.len(), max, overflow, classify_host_peer(&stream)) {
                    // Say so, rather than dropping the peer silently. The guest
                    // retries on its own; a silent drop looks like a dead host.
                    tracing::warn!(
                        "refusing connection: {} concurrent clients already in flight",
                        handles.len()
                    );
                    let _ = write_frame(
                        &mut stream,
                        &HostMessage::Error {
                            reason: "busy: too many concurrent connections".into(),
                            code: Some("busy".into()),
                        },
                    );
                    continue;
                }

                let cfg = config.clone();
                let state = Arc::clone(state);
                let handle = std::thread::spawn(move || {
                    if let Err(e) = handle_connection(&mut stream, &cfg, &state, &SystemActions) {
                        tracing::error!("error handling connection: {}", e);
                    }
                });
                handles.push(handle);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(200));
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

/// Run the host socket server for a container.
pub fn run(socket_path: &Path, config: &Config, container_name: &str) -> anyhow::Result<()> {
    // SIGTERM/SIGINT set this flag; the accept loop polls it on its tick.
    let shutdown = crate::protocol::sysutil::install_shutdown_signal_handler();

    let config = config.clone();
    let path = socket_path.to_path_buf();
    let idle_timeout_secs = parse_idle_timeout_secs(&config.lifecycle.idle_timeout);

    let activation_fd = listen_fd();
    let was_socket_activated = activation_fd.is_some();
    let listener = match activation_fd {
        Some(fd) => {
            // SAFETY: `fd` comes from systemd's `LISTEN_FDS` activation
            // protocol: the user manager hands over ownership of a valid
            // listening socket fd, which this process must adopt exactly
            // once. No safe wrapper exists for externally-sourced fds.
            // `set_cloexec` runs before any child is spawned, so host-exec
            // commands, `xdg-open` and `wl-copy` never inherit the listener.
            #[allow(unsafe_code)]
            let listener = unsafe { UnixListener::from_raw_fd(fd) };
            crate::process::set_cloexec(listener.as_fd())?;
            listener
        }
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

    accept_loop(
        listener,
        &config,
        &state,
        &shutdown,
        &peer_is_in_host_userns,
        MAX_CONCURRENT,
        HOST_OVERFLOW,
    )
}

#[cfg(test)]
mod tests {
    use super::conn::{has_cap, note_failure};
    use super::handlers::{MAX_URI_LEN, UriError, validate_host_exec_args, validate_uri};
    use crate::config::types::DEFAULT_ALLOWED_URI_SCHEMES;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

    // ── validate_uri tests ──

    /// The default (unconfigured) allowlist shape used by every request
    /// before the operator widens `integration.xdg_open.allowed_schemes`.
    fn default_schemes() -> Vec<String> {
        DEFAULT_ALLOWED_URI_SCHEMES
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn ok(uri: &str) -> String {
        validate_uri(uri, &default_schemes())
            .unwrap_or_else(|e| panic!("{uri:?} should be accepted: {e}"))
    }

    #[test]
    fn allows_http_https_mailto() {
        assert_eq!(ok("https://example.com"), "https://example.com");
        assert_eq!(ok("http://example.com"), "http://example.com");
        assert_eq!(ok("mailto:a@b"), "mailto:a@b");
    }

    #[test]
    fn refuses_schemes_outside_the_allowlist() {
        for uri in [
            "smb://attacker/share",
            "archive://file%3A%2F%2F%2Fhome%2Fuser",
            "trash:///",
            "recent:///",
            "computer:///",
            "steam://run/1",
            "vscode://file/etc/passwd",
            "sftp://host/",
            "davs://host/",
            "file:///etc/passwd",
            "FILE:///etc/passwd",
            "javascript:alert(1)",
            "ms-settings:display",
        ] {
            assert!(
                matches!(
                    validate_uri(uri, &default_schemes()),
                    Err(UriError::SchemeNotAllowed { .. })
                ),
                "{uri:?} should be refused by scheme"
            );
        }
    }

    #[test]
    fn configured_scheme_is_accepted_and_the_refusal_names_it() {
        let schemes = vec!["smb".to_string(), "http".to_string()];
        assert_eq!(
            validate_uri("smb://host/share", &schemes).unwrap(),
            "smb://host/share",
            "an explicitly configured scheme must open"
        );
        // The refusal lists the configured allowlist, not the built-in one.
        let err = validate_uri("steam://run/1", &schemes).unwrap_err();
        let UriError::SchemeNotAllowed { scheme, allowed } = err else {
            panic!("expected a scheme refusal, got {err:?}")
        };
        assert_eq!(scheme, "steam");
        assert_eq!(allowed, schemes);
        // Still case-insensitive for a configured entry.
        assert!(validate_uri("SMB://host/share", &schemes).is_ok());
    }

    #[test]
    fn web_urls_without_a_host_are_refused_for_any_configured_list() {
        let schemes = vec!["smb".to_string()];
        // A configured non-web scheme does not need a host...
        assert_eq!(
            validate_uri("smb:mailbox", &schemes).unwrap(),
            "smb:mailbox"
        );
        // ...but http/https always do, web or not configured.
        assert_eq!(
            validate_uri("https://", &default_schemes()),
            Err(UriError::Malformed)
        );
    }

    #[test]
    fn refuses_paths_and_empty_input() {
        assert_eq!(
            validate_uri("/etc/passwd", &default_schemes()),
            Err(UriError::Malformed)
        );
        assert_eq!(
            validate_uri("../foo", &default_schemes()),
            Err(UriError::Malformed)
        );
        assert_eq!(validate_uri("", &default_schemes()), Err(UriError::Empty));
        assert_eq!(
            validate_uri("   ", &default_schemes()),
            Err(UriError::Empty)
        );
    }

    #[test]
    fn refuses_control_characters_and_oversized_input() {
        assert_eq!(
            validate_uri("https://example.com/\nrm", &default_schemes()),
            Err(UriError::ControlCharacter)
        );
        assert_eq!(
            validate_uri("https://example.com/\0", &default_schemes()),
            Err(UriError::ControlCharacter)
        );
        let long = format!("https://example.com/{}", "a".repeat(MAX_URI_LEN));
        assert_eq!(
            validate_uri(&long, &default_schemes()),
            Err(UriError::TooLong)
        );
    }

    #[test]
    fn refuses_web_urls_without_a_host() {
        assert_eq!(
            validate_uri("https://", &default_schemes()),
            Err(UriError::Malformed)
        );
    }

    #[test]
    fn wraps_bare_domain() {
        assert_eq!(ok("example.com"), "https://example.com");
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(ok("  https://example.com  "), "https://example.com");
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
            assert!(matches!(
                msg,
                crate::protocol::HostMessage::Error { reason, .. } if reason == "probe"
            ));
        }
    }

    // ── connection admission tests ──

    #[test]
    fn guest_pool_is_capped_and_host_peers_use_reserved_slots() {
        // max = 4 shared slots, 2 reserved for host-side peers.
        assert!(super::connection_allowed(0, 4, 2, false));
        assert!(super::connection_allowed(3, 4, 2, false));
        assert!(!super::connection_allowed(4, 4, 2, false));
        assert!(!super::connection_allowed(9, 4, 2, false));
        assert!(super::connection_allowed(4, 4, 2, true));
        assert!(super::connection_allowed(5, 4, 2, true));
        assert!(!super::connection_allowed(6, 4, 2, true));
    }

    /// Drive the real accept loop with a scripted guest/host peer mix: with
    /// one shared slot, the first guest is accepted, the second is told the
    /// pool is busy, a host-side peer still gets a reserved slot, and a
    /// freed slot admits guests again.
    #[test]
    fn host_peer_is_accepted_when_the_guest_pool_is_full() {
        use std::os::unix::net::{UnixListener, UnixStream};
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("serve.sock");
        let listener = UnixListener::bind(&path).unwrap();

        let config = crate::config::Config::embedded();
        let state = Arc::new(super::SharedState {
            session_count: AtomicU32::new(0),
            container_name: "dev".into(),
            idle_timeout_secs: 0,
            was_socket_activated: false,
        });
        let shutdown = Arc::new(AtomicBool::new(false));
        let next = AtomicUsize::new(0);
        // Peer class per accepted connection, in order.
        let classes = [false, false, true, false];
        let classify = move |_: &UnixStream| {
            let i = next.fetch_add(1, Ordering::SeqCst);
            classes[i.min(classes.len() - 1)]
        };

        let shutdown_loop = Arc::clone(&shutdown);
        let worker = std::thread::spawn(move || {
            super::accept_loop(listener, &config, &state, &shutdown_loop, &classify, 1, 1)
        });

        fn connect(path: &std::path::Path) -> UnixStream {
            let stream = UnixStream::connect(path).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            stream
        }

        fn hello(stream: &mut UnixStream) {
            crate::protocol::write_frame(
                stream,
                &crate::protocol::GuestMessage::Hello {
                    protocol_version: crate::protocol::PROTOCOL_VERSION,
                    guest_version: "test".into(),
                    container: "dev".into(),
                    capabilities: vec![],
                },
            )
            .unwrap();
        }

        fn reply_msg(stream: &mut UnixStream) -> crate::protocol::HostMessage {
            let bytes = crate::protocol::read_frame(stream)
                .unwrap()
                .expect("server closed the connection");
            serde_json::from_slice(&bytes).unwrap()
        }

        fn reply_kind(stream: &mut UnixStream) -> String {
            match reply_msg(stream) {
                crate::protocol::HostMessage::HelloAck { .. } => "ack".into(),
                crate::protocol::HostMessage::Error { reason, .. } => format!("error:{reason}"),
                other => panic!("unexpected reply: {other:?}"),
            }
        }

        // One shared slot: the first guest holds it.
        let mut guest = connect(&path);
        hello(&mut guest);
        assert_eq!(reply_kind(&mut guest), "ack");

        // The pool is full: the next guest is refused with a busy reason
        // and a typed `busy` code so clients can branch without parsing.
        let mut refused = connect(&path);
        hello(&mut refused);
        let reply = reply_msg(&mut refused);
        match reply {
            crate::protocol::HostMessage::Error { reason, code } => {
                assert!(
                    reason.contains("busy"),
                    "expected a busy refusal, got {reason:?}"
                );
                assert_eq!(code.as_deref(), Some("busy"));
            }
            other => panic!("expected an error frame, got {other:?}"),
        }
        drop(refused);

        // A host-side peer still gets a reserved slot.
        let mut host_peer = connect(&path);
        hello(&mut host_peer);
        assert_eq!(reply_kind(&mut host_peer), "ack");

        // Both slots in use: another guest is refused.
        let mut refused2 = connect(&path);
        hello(&mut refused2);
        assert!(reply_kind(&mut refused2).contains("busy"));
        drop(refused2);

        // Free both slots; a guest is admitted again once the server-side
        // threads observe the EOF (poll — thread teardown is asynchronous).
        drop(guest);
        drop(host_peer);
        let began = std::time::Instant::now();
        loop {
            let mut retry = connect(&path);
            hello(&mut retry);
            if reply_kind(&mut retry) == "ack" {
                drop(retry);
                break;
            }
            assert!(
                began.elapsed() < Duration::from_secs(10),
                "freed slot was never reused"
            );
            std::thread::sleep(Duration::from_millis(100));
        }

        shutdown.store(true, Ordering::Relaxed);
        worker.join().expect("accept loop panicked").unwrap();
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
        config.integration.xdg_open.enabled = false;
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

    // ── streaming host-exec helper tests ──

    #[test]
    fn signal_death_maps_to_the_shell_convention() {
        use super::handlers::exit_code_and_signal;
        let killed = std::process::Command::new("sh")
            .args(["-c", "kill -TERM $$"])
            .status()
            .unwrap();
        assert_eq!(exit_code_and_signal(&killed), (143, Some(15)));
        let clean = std::process::Command::new("sh")
            .args(["-c", "exit 42"])
            .status()
            .unwrap();
        assert_eq!(exit_code_and_signal(&clean), (42, None));
    }

    /// A child that never stops writing is killed once its total output
    /// passes the cap, and the guest gets a truncation note plus a failure
    /// status instead of unbounded frames.
    #[test]
    fn output_cap_kills_a_chatty_child_and_reports_truncation() {
        use super::handlers::stream_child_output;
        let (mut server, mut client) = socket_pair();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let child = std::process::Command::new("yes")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        stream_child_output(
            &mut server,
            child,
            std::time::Duration::from_secs(30),
            20_000,
        )
        .unwrap();
        drop(server);

        // Zero or more chunks under the cap, then the truncation note,
        // then done-with-failure. Chunk sizes depend on pipe timing, so
        // only the shape and the totals are asserted.
        let mut forwarded = 0usize;
        let mut frames = 0;
        loop {
            frames += 1;
            assert!(frames < 100, "cap never fired");
            let bytes = crate::protocol::read_frame(&mut client)
                .unwrap()
                .expect("truncation report");
            let msg: crate::protocol::HostMessage = serde_json::from_slice(&bytes).unwrap();
            match msg {
                crate::protocol::HostMessage::HostExecOutput { data, .. } => {
                    forwarded += data.len();
                    assert!(forwarded <= 20_000, "cap exceeded: {forwarded}");
                }
                crate::protocol::HostMessage::HostExecStderr { data } => {
                    assert!(data.contains("truncated"), "{data:?}");
                    break;
                }
                other => panic!("expected output or truncation, got {other:?}"),
            }
        }
        let bytes = crate::protocol::read_frame(&mut client)
            .unwrap()
            .expect("done frame");
        let msg: crate::protocol::HostMessage = serde_json::from_slice(&bytes).unwrap();
        assert!(
            matches!(
                msg,
                crate::protocol::HostMessage::HostExecDone { exit_code: 1, .. }
            ),
            "expected failure status, got {msg:?}"
        );
    }
}
