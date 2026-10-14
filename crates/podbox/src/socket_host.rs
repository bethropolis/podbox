use std::os::fd::FromRawFd;
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
use monitor::{listen_fd, setup_signal_handler};

/// Maximum consecutive failed negotiations (bad hello, unauthenticated
/// privileged message, malformed frame) before the connection is dropped.
const MAX_NEGOTIATION_FAILURES: u32 = 5;

/// Max number of concurrent host threads handling guest connections.
const MAX_CONCURRENT: usize = 4;

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

/// Run the host socket server for a container.
pub fn run(socket_path: &Path, config: &Config, container_name: &str) -> anyhow::Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));
    setup_signal_handler(&shutdown)?;

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
            #[allow(unsafe_code)]
            unsafe {
                UnixListener::from_raw_fd(fd)
            }
        }
        None => {
            let _ = std::fs::remove_file(&path);
            UnixListener::bind(&path)?
        }
    };
    // Non-blocking + periodic tick so SIGTERM/SIGINT ends the accept loop
    // promptly instead of blocking in accept(2) until systemd's
    // TimeoutStopSec SIGKILL.
    listener.set_nonblocking(true)?;

    let state = Arc::new(SharedState {
        session_count: AtomicU32::new(0),
        container_name: container_name.to_string(),
        idle_timeout_secs,
        was_socket_activated,
    });

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

                if handles.len() >= MAX_CONCURRENT {
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
                        },
                    );
                    continue;
                }

                let cfg = config.clone();
                let state = Arc::clone(&state);
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

#[cfg(test)]
mod tests {
    use super::conn::{has_cap, note_failure};
    use super::handlers::{MAX_URI_LEN, UriError, validate_host_exec_args, validate_uri};
    use std::collections::HashSet;

    // ── validate_uri tests ──

    fn ok(uri: &str) -> String {
        validate_uri(uri).unwrap_or_else(|e| panic!("{uri:?} should be accepted: {e}"))
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
                matches!(validate_uri(uri), Err(UriError::SchemeNotAllowed(_))),
                "{uri:?} should be refused by scheme"
            );
        }
    }

    #[test]
    fn refuses_paths_and_empty_input() {
        assert_eq!(validate_uri("/etc/passwd"), Err(UriError::Malformed));
        assert_eq!(validate_uri("../foo"), Err(UriError::Malformed));
        assert_eq!(validate_uri(""), Err(UriError::Empty));
        assert_eq!(validate_uri("   "), Err(UriError::Empty));
    }

    #[test]
    fn refuses_control_characters_and_oversized_input() {
        assert_eq!(
            validate_uri("https://example.com/\nrm"),
            Err(UriError::ControlCharacter)
        );
        assert_eq!(
            validate_uri("https://example.com/\0"),
            Err(UriError::ControlCharacter)
        );
        let long = format!("https://example.com/{}", "a".repeat(MAX_URI_LEN));
        assert_eq!(validate_uri(&long), Err(UriError::TooLong));
    }

    #[test]
    fn refuses_web_urls_without_a_host() {
        assert_eq!(validate_uri("https://"), Err(UriError::Malformed));
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
