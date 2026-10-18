//! Guest↔host socket protocol, driven through the real connection loop.
//!
//! Every test runs [`podbox::socket_host::serve_connection`] (the same loop
//! `podbox serve` runs per connection) over a `UnixStream::pair()`. Host side
//! effects go to a recording fake, so nothing touches the real desktop.

use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use podbox::config::{Config, HostExecEntry};
use podbox::protocol::{
    CAP_CLIPBOARD, CAP_HOST_EXEC, CAP_HOST_EXEC_STREAM, CAP_NOTIFY, CAP_XDG_OPEN, ExecStream,
    GuestMessage, HostMessage, NotifyAction, PROTOCOL_VERSION, read_frame, write_frame,
};
use podbox::socket_host::{HostActions, serve_connection};

/// Records every host side effect. Clipboard is a single slot, as on a desktop.
#[derive(Default)]
struct Recorder {
    calls: Mutex<Vec<String>>,
    clipboard: Mutex<String>,
    /// Action key returned from `notify` when actions are offered.
    chosen_key: String,
}

impl Recorder {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl HostActions for Recorder {
    fn notify(
        &self,
        summary: &str,
        _body: &str,
        urgency: &str,
        app_name: &str,
        actions: &[NotifyAction],
    ) -> anyhow::Result<String> {
        self.calls.lock().unwrap().push(format!(
            "notify:{summary}:{urgency}:{app_name}:{}",
            actions.len()
        ));
        Ok(self.chosen_key.clone())
    }

    fn open_uri(&self, uri: &str) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(format!("open:{uri}"));
        Ok(())
    }

    fn clipboard_set(&self, text: &str) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push("clipboard_set".into());
        *self.clipboard.lock().unwrap() = text.to_string();
        Ok(())
    }

    fn clipboard_get(&self) -> anyhow::Result<String> {
        self.calls.lock().unwrap().push("clipboard_get".into());
        Ok(self.clipboard.lock().unwrap().clone())
    }

    fn stop_unit(&self, name: &str) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(format!("stop:{name}"));
        Ok(())
    }
}

/// A config with every integration switched on and a two-entry host-exec
/// allowlist. Absolute paths, so `canonicalize` works without PATH lookup.
fn enabled_config() -> Config {
    let mut cfg = Config::embedded();
    cfg.integration.notify = true;
    cfg.integration.xdg_open.enabled = true;
    cfg.integration.clipboard = true;
    cfg.integration.host_exec.enabled = true;
    cfg.integration.host_exec.allowlist = Some(BTreeMap::from([
        (
            "echo".to_string(),
            HostExecEntry::Simple("/bin/echo".into()),
        ),
        ("sh".to_string(), HostExecEntry::Simple("/bin/sh".into())),
    ]));
    cfg
}

/// Run the real loop on one end of a socket pair. Returns the guest end and
/// the server thread, whose result shows how the connection ended.
fn start(cfg: Config, rec: Arc<Recorder>) -> (UnixStream, JoinHandle<anyhow::Result<()>>) {
    let (client, mut server) = UnixStream::pair().unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let handle = std::thread::spawn(move || serve_connection(&mut server, &cfg, "dev", &*rec));
    (client, handle)
}

/// Close the guest end and wait for the server loop to return.
fn finish(client: UnixStream, handle: JoinHandle<anyhow::Result<()>>) -> anyhow::Result<()> {
    drop(client);
    handle.join().expect("server thread panicked")
}

fn send(c: &mut UnixStream, msg: &GuestMessage) {
    write_frame(c, msg).unwrap();
}

fn recv(c: &mut UnixStream) -> HostMessage {
    let bytes = read_frame(c)
        .unwrap()
        .expect("server closed the connection");
    serde_json::from_slice(&bytes).unwrap()
}

fn hello(caps: &[&str]) -> GuestMessage {
    GuestMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        guest_version: "test".into(),
        container: "dev".into(),
        capabilities: caps.iter().map(|c| (*c).to_string()).collect(),
    }
}

fn notify(summary: &str, actions: Vec<NotifyAction>) -> GuestMessage {
    GuestMessage::Notify {
        summary: summary.into(),
        body: "body".into(),
        urgency: "normal".into(),
        actions,
        app_name: String::new(),
    }
}

fn expect_error(msg: HostMessage, needle: &str) {
    match msg {
        HostMessage::Error { reason, .. } => {
            assert!(
                reason.contains(needle),
                "reason {reason:?} lacks {needle:?}"
            );
        }
        other => panic!("expected Error containing {needle:?}, got {other:?}"),
    }
}

/// Write a length-prefixed frame by hand, so a test can send bad framing.
fn write_raw(c: &mut UnixStream, len: u32, payload: &[u8]) {
    c.write_all(&len.to_be_bytes()).unwrap();
    c.write_all(payload).unwrap();
    c.flush().unwrap();
}

#[test]
fn hello_grants_requested_capabilities_and_names_unknown_ones() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello(&[CAP_NOTIFY, "bogus"]));
    match recv(&mut c) {
        HostMessage::HelloAck {
            accepted, rejected, ..
        } => {
            assert_eq!(accepted, vec![CAP_NOTIFY.to_string()]);
            assert_eq!(rejected, vec!["bogus".to_string()]);
        }
        other => panic!("expected HelloAck, got {other:?}"),
    }
    finish(c, h).unwrap();
}

#[test]
fn capability_disabled_by_config_is_rejected_in_the_ack() {
    let mut cfg = enabled_config();
    cfg.integration.notify = false;
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello(&[CAP_NOTIFY]));
    match recv(&mut c) {
        HostMessage::HelloAck {
            accepted, rejected, ..
        } => {
            assert!(accepted.is_empty());
            assert_eq!(rejected, vec![CAP_NOTIFY.to_string()]);
        }
        other => panic!("expected HelloAck, got {other:?}"),
    }
    finish(c, h).unwrap();
}

#[test]
fn protocol_version_mismatch_shuts_down_and_grants_nothing() {
    let rec = Arc::new(Recorder::default());
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    send(
        &mut c,
        &GuestMessage::Hello {
            protocol_version: PROTOCOL_VERSION + 1,
            guest_version: "test".into(),
            container: "dev".into(),
            capabilities: vec![CAP_NOTIFY.into()],
        },
    );
    assert!(matches!(recv(&mut c), HostMessage::Shutdown));
    expect_error(recv(&mut c), "hello rejected");
    send(&mut c, &notify("sneaky", vec![]));
    expect_error(recv(&mut c), "not accepted");
    finish(c, h).unwrap();
    assert!(rec.calls().is_empty(), "{:?}", rec.calls());
}

#[test]
fn requests_before_hello_are_refused_and_never_run() {
    let rec = Arc::new(Recorder::default());
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    send(&mut c, &notify("before hello", vec![]));
    expect_error(recv(&mut c), "not accepted");
    send(
        &mut c,
        &GuestMessage::XdgOpen {
            uri: "https://example.com".into(),
        },
    );
    expect_error(recv(&mut c), "not accepted");
    send(&mut c, &GuestMessage::ClipboardSet { text: "x".into() });
    expect_error(recv(&mut c), "not accepted");
    finish(c, h).unwrap();
    assert!(rec.calls().is_empty(), "{:?}", rec.calls());
}

#[test]
fn granted_notify_reaches_the_backend_without_a_reply() {
    let rec = Arc::new(Recorder::default());
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    send(&mut c, &hello(&[CAP_NOTIFY]));
    let _ = recv(&mut c);
    send(&mut c, &notify("hi", vec![]));
    finish(c, h).unwrap();
    assert_eq!(rec.calls(), vec!["notify:hi:normal::0".to_string()]);
}

#[test]
fn notify_urgency_and_app_name_reach_the_backend() {
    let rec = Arc::new(Recorder::default());
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    send(&mut c, &hello(&[CAP_NOTIFY]));
    let _ = recv(&mut c);
    send(
        &mut c,
        &GuestMessage::Notify {
            summary: "Title".into(),
            body: "Body".into(),
            urgency: "critical".into(),
            actions: vec![],
            app_name: "Files".into(),
        },
    );
    finish(c, h).unwrap();
    assert_eq!(
        rec.calls(),
        vec!["notify:Title:critical:Files:0".to_string()]
    );
}

#[test]
fn notify_with_actions_returns_the_chosen_key() {
    let rec = Arc::new(Recorder {
        chosen_key: "open".into(),
        ..Recorder::default()
    });
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    send(&mut c, &hello(&[CAP_NOTIFY]));
    let _ = recv(&mut c);
    send(
        &mut c,
        &notify(
            "pick",
            vec![NotifyAction {
                key: "open".into(),
                label: "Open".into(),
            }],
        ),
    );
    match recv(&mut c) {
        HostMessage::NotifyActionResult { action_key, .. } => assert_eq!(action_key, "open"),
        other => panic!("expected NotifyActionResult, got {other:?}"),
    }
    finish(c, h).unwrap();
}

/// The host closes after one xdg-open request, so each request gets its own
/// connection, as the one-shot guest interceptor sends it.
fn xdg_open_once(uri: &str, rec: Arc<Recorder>) -> anyhow::Result<()> {
    let (mut c, h) = start(enabled_config(), rec);
    send(&mut c, &hello(&[CAP_XDG_OPEN]));
    let _ = recv(&mut c);
    send(&mut c, &GuestMessage::XdgOpen { uri: uri.into() });
    let result = h.join().expect("server thread panicked");
    drop(c);
    result
}

#[test]
fn xdg_open_refuses_schemes_outside_the_allowlist_with_a_reason() {
    for uri in [
        "file:///etc/passwd",
        "/etc/passwd",
        "smb://attacker/share",
        "archive://file%3A%2F%2F%2Fhome%2Fuser",
        "trash:///",
        "steam://run/1",
        "vscode://file/etc/passwd",
    ] {
        let rec = Arc::new(Recorder::default());
        let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
        send(&mut c, &hello(&[CAP_XDG_OPEN]));
        let _ = recv(&mut c);
        send(&mut c, &GuestMessage::XdgOpen { uri: uri.into() });
        match recv(&mut c) {
            HostMessage::Error { reason, .. } => {
                assert!(reason.starts_with("xdg-open refused"), "{uri}: {reason}");
            }
            other => panic!("{uri}: expected a refusal, got {other:?}"),
        }
        finish(c, h).unwrap();
        assert!(rec.calls().is_empty(), "{uri} reached the backend");
    }
}

#[test]
fn xdg_open_hands_an_allowed_uri_to_the_backend_then_closes_quietly() {
    let rec = Arc::new(Recorder::default());
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    send(&mut c, &hello(&[CAP_XDG_OPEN]));
    let _ = recv(&mut c);
    send(
        &mut c,
        &GuestMessage::XdgOpen {
            uri: "https://example.com".into(),
        },
    );
    // Success is the close, with no reply frame.
    h.join().expect("server thread panicked").unwrap();
    assert!(read_frame(&mut c).unwrap().is_none(), "unexpected reply");
    assert_eq!(rec.calls(), vec!["open:https://example.com".to_string()]);
}

#[test]
fn xdg_open_accepts_mailto_and_bare_domains() {
    for uri in ["mailto:a@b", "example.com"] {
        let rec = Arc::new(Recorder::default());
        xdg_open_once(uri, Arc::clone(&rec)).unwrap();
        assert_eq!(rec.calls().len(), 1, "{uri} was not opened");
    }
}

/// A scheme outside the built-in allowlist opens once the operator names it
/// in `integration.xdg_open.allowed_schemes`; everything else stays refused.
#[test]
fn configured_allowed_schemes_open_and_others_stay_refused() {
    let rec = Arc::new(Recorder::default());
    let mut cfg = enabled_config();
    cfg.integration.xdg_open.allowed_schemes = vec!["smb".to_string()];
    {
        let (mut c, h) = start(cfg.clone(), Arc::clone(&rec));
        send(&mut c, &hello(&[CAP_XDG_OPEN]));
        let _ = recv(&mut c);
        send(
            &mut c,
            &GuestMessage::XdgOpen {
                uri: "smb://host/share".into(),
            },
        );
        h.join().expect("server thread panicked").unwrap();
        assert!(read_frame(&mut c).unwrap().is_none(), "unexpected reply");
        assert_eq!(rec.calls().len(), 1, "configured scheme was not opened");
    }
    // The configured list is the whole allowlist: the defaults that are not
    // listed are refused by the same rule.
    let reason = {
        let (mut c, h) = start(cfg, Arc::clone(&rec));
        send(&mut c, &hello(&[CAP_XDG_OPEN]));
        let _ = recv(&mut c);
        send(
            &mut c,
            &GuestMessage::XdgOpen {
                uri: "https://example.com".into(),
            },
        );
        let msg = recv(&mut c);
        finish(c, h).unwrap();
        match msg {
            HostMessage::Error { reason, .. } => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    };
    assert!(
        reason.contains("allowed: smb"),
        "refusal should name only the configured list: {reason}"
    );
}

#[test]
fn clipboard_set_then_get_round_trips_through_the_backend() {
    let rec = Arc::new(Recorder::default());
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    send(&mut c, &hello(&[CAP_CLIPBOARD]));
    let _ = recv(&mut c);
    send(
        &mut c,
        &GuestMessage::ClipboardSet {
            text: "hello there".into(),
        },
    );
    send(&mut c, &GuestMessage::ClipboardGet);
    match recv(&mut c) {
        HostMessage::ClipboardData { text } => assert_eq!(text, "hello there"),
        other => panic!("expected ClipboardData, got {other:?}"),
    }
    finish(c, h).unwrap();
}

#[test]
fn clipboard_is_refused_without_its_capability() {
    let rec = Arc::new(Recorder::default());
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    send(&mut c, &hello(&[CAP_NOTIFY]));
    let _ = recv(&mut c);
    send(&mut c, &GuestMessage::ClipboardGet);
    expect_error(recv(&mut c), "'clipboard' not accepted");
    finish(c, h).unwrap();
    assert!(rec.calls().is_empty());
}

#[test]
fn clipboard_round_trip_preserves_whitespace_exactly() {
    let rec = Arc::new(Recorder::default());
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    send(&mut c, &hello(&[CAP_CLIPBOARD]));
    let _ = recv(&mut c);
    send(
        &mut c,
        &GuestMessage::ClipboardSet {
            text: "  indented\n\n".into(),
        },
    );
    send(&mut c, &GuestMessage::ClipboardGet);
    match recv(&mut c) {
        HostMessage::ClipboardData { text } => assert_eq!(text, "  indented\n\n"),
        other => panic!("expected ClipboardData, got {other:?}"),
    }
    finish(c, h).unwrap();
}

#[test]
fn oversized_clipboard_get_is_an_error_and_leaves_the_session_usable() {
    use podbox::protocol::MAX_CLIPBOARD_BYTES;
    let rec = Arc::new(Recorder::default());
    *rec.clipboard.lock().unwrap() = "x".repeat(MAX_CLIPBOARD_BYTES + 1);
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    send(&mut c, &hello(&[CAP_CLIPBOARD]));
    let _ = recv(&mut c);
    send(&mut c, &GuestMessage::ClipboardGet);
    expect_error(recv(&mut c), "exceeds");
    // The session survives: smaller content flows afterwards.
    *rec.clipboard.lock().unwrap() = "small".to_string();
    send(&mut c, &GuestMessage::ClipboardGet);
    match recv(&mut c) {
        HostMessage::ClipboardData { text } => assert_eq!(text, "small"),
        other => panic!("expected ClipboardData, got {other:?}"),
    }
    finish(c, h).unwrap();
}

#[test]
fn oversized_clipboard_set_is_an_error_and_leaves_the_session_usable() {
    use podbox::protocol::MAX_CLIPBOARD_BYTES;
    let rec = Arc::new(Recorder::default());
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    send(&mut c, &hello(&[CAP_CLIPBOARD]));
    let _ = recv(&mut c);
    send(
        &mut c,
        &GuestMessage::ClipboardSet {
            text: "x".repeat(MAX_CLIPBOARD_BYTES + 1),
        },
    );
    expect_error(recv(&mut c), "exceeds");
    assert!(rec.calls().is_empty(), "oversized set reached the backend");
    // The session survives: a normal round trip works afterwards.
    send(
        &mut c,
        &GuestMessage::ClipboardSet {
            text: "fine".into(),
        },
    );
    send(&mut c, &GuestMessage::ClipboardGet);
    match recv(&mut c) {
        HostMessage::ClipboardData { text } => assert_eq!(text, "fine"),
        other => panic!("expected ClipboardData, got {other:?}"),
    }
    finish(c, h).unwrap();
}

#[test]
fn host_exec_runs_an_allowlisted_command_and_reports_its_exit() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello(&[CAP_HOST_EXEC]));
    let _ = recv(&mut c);
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "echo".into(),
            args: vec!["hello from test".into()],
        },
    );
    match recv(&mut c) {
        HostMessage::HostExecStdout { data } => {
            assert!(data.contains("hello from test"), "{data:?}")
        }
        other => panic!("expected HostExecStdout, got {other:?}"),
    }
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "sh".into(),
            args: vec!["-c".into(), "exit 42".into()],
        },
    );
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 42, .. }
    ));
    finish(c, h).unwrap();
}

#[test]
fn host_exec_refuses_commands_outside_the_allowlist() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello(&[CAP_HOST_EXEC]));
    let _ = recv(&mut c);
    for cmd in ["rm", "/bin/rm", "echo/../rm"] {
        send(
            &mut c,
            &GuestMessage::HostExec {
                cmd: cmd.into(),
                args: vec!["-rf".into(), "/tmp/x".into()],
            },
        );
        match recv(&mut c) {
            HostMessage::HostExecStderr { data } => {
                assert!(data.contains("Permission denied"), "{cmd}: {data:?}");
            }
            other => panic!("{cmd}: expected stderr refusal, got {other:?}"),
        }
        assert!(matches!(
            recv(&mut c),
            HostMessage::HostExecDone { exit_code: 1, .. }
        ));
    }
    finish(c, h).unwrap();
}

#[test]
fn host_exec_refuses_argument_injection_patterns() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello(&[CAP_HOST_EXEC]));
    let _ = recv(&mut c);
    for args in [
        vec!["a;id".to_string()],
        vec!["$(id)".to_string()],
        vec!["--config=/tmp/evil".to_string()],
        vec!["x > /etc/passwd".to_string()],
    ] {
        send(
            &mut c,
            &GuestMessage::HostExec {
                cmd: "echo".into(),
                args: args.clone(),
            },
        );
        match recv(&mut c) {
            HostMessage::HostExecStderr { data } => {
                assert!(data.contains("Security violation"), "{args:?}: {data:?}");
            }
            other => panic!("{args:?}: expected refusal, got {other:?}"),
        }
        assert!(matches!(
            recv(&mut c),
            HostMessage::HostExecDone { exit_code: 1, .. }
        ));
    }
    finish(c, h).unwrap();
}

#[test]
fn host_exec_is_refused_when_disabled_by_config() {
    let mut cfg = enabled_config();
    cfg.integration.host_exec.enabled = false;
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello(&[CAP_HOST_EXEC]));
    match recv(&mut c) {
        HostMessage::HelloAck { accepted, .. } => assert!(accepted.is_empty()),
        other => panic!("expected HelloAck, got {other:?}"),
    }
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "echo".into(),
            args: vec![],
        },
    );
    expect_error(recv(&mut c), "'host_exec' not accepted");
    finish(c, h).unwrap();
}

#[test]
fn oversized_frame_is_rejected_before_the_payload_is_read() {
    let rec = Arc::new(Recorder::default());
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    // 16 MiB + 1 is one byte over MAX_FRAME_SIZE. No payload follows.
    c.write_all(&(16u32 * 1024 * 1024 + 1).to_be_bytes())
        .unwrap();
    let result = h.join().expect("server thread panicked");
    drop(c);
    assert!(result.is_err(), "oversized frame was accepted");
    assert!(rec.calls().is_empty());
}

#[test]
fn truncated_frame_then_eof_ends_the_connection_with_an_error() {
    let rec = Arc::new(Recorder::default());
    let (mut c, h) = start(enabled_config(), Arc::clone(&rec));
    // Promise 100 bytes, send 10, then close.
    write_raw(&mut c, 100, &[b'x'; 10]);
    drop(c);
    let result = h.join().expect("server thread panicked");
    assert!(result.is_err(), "truncated frame was accepted");
    assert!(rec.calls().is_empty());
}

#[test]
fn malformed_json_is_reported_and_the_session_survives() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    write_raw(&mut c, 9, b"{not json");
    expect_error(recv(&mut c), "malformed frame");
    send(&mut c, &hello(&[CAP_NOTIFY]));
    assert!(matches!(recv(&mut c), HostMessage::HelloAck { .. }));
    finish(c, h).unwrap();
}

#[test]
fn silent_connection_is_closed_after_the_hello_deadline() {
    // The server's HELLO_DEADLINE is 5 s. Asserted loosely so a slow CI host
    // cannot flake it.
    let (mut c, h) = start(enabled_config(), Arc::default());
    let began = std::time::Instant::now();
    expect_error(recv(&mut c), "hello deadline");
    finish(c, h).unwrap();
    let waited = began.elapsed();
    assert!(
        waited >= Duration::from_secs(4),
        "closed too early: {waited:?}"
    );
    assert!(
        waited < Duration::from_secs(20),
        "closed too late: {waited:?}"
    );
}

#[test]
fn partial_frame_across_a_timeout_does_not_desync() {
    // Two bytes of the length prefix, then a pause longer than the server's
    // pre-Hello read timeout (5 s) but shorter than the frame deadline (10 s),
    // then the rest. The frame must arrive intact: the Hello is acknowledged,
    // and no payload byte is read as a length prefix.
    let (mut c, h) = start(enabled_config(), Arc::default());
    let mut frame = Vec::new();
    write_frame(&mut frame, &hello(&[CAP_NOTIFY])).unwrap();
    c.write_all(&frame[..2]).unwrap();
    std::thread::sleep(Duration::from_secs(6));
    c.write_all(&frame[2..]).unwrap();
    match recv(&mut c) {
        HostMessage::HelloAck { accepted, .. } => {
            assert_eq!(accepted, vec![CAP_NOTIFY.to_string()])
        }
        other => panic!("expected HelloAck after the split frame, got {other:?}"),
    }
    finish(c, h).unwrap();
}

// ── Spec 001 acceptance: the real guest negotiates, the real host loop serves.
//
// These go through `podbox_guest::socket::negotiate_at`, the same call the
// interceptors make. If a guest sends a request without `Hello` again, the
// host rejects it and the recorder stays empty, so these tests fail.

/// Listen on a temp socket and serve exactly one accepted connection with the
/// real loop. The returned path is what the guest connects to.
fn serve_one_guest(
    cfg: Config,
    rec: Arc<Recorder>,
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    JoinHandle<anyhow::Result<()>>,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dev.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        serve_connection(&mut stream, &cfg, "dev", &*rec)
    });
    (dir, path, handle)
}

#[test]
fn guest_one_shot_requests_succeed_when_capabilities_are_granted() {
    let rec = Arc::new(Recorder::default());
    let (_dir, path, server) = serve_one_guest(enabled_config(), Arc::clone(&rec));

    let mut guest = podbox_guest::socket::negotiate_at(
        &path,
        "dev",
        &[CAP_NOTIFY, CAP_XDG_OPEN, CAP_CLIPBOARD],
    )
    .expect("negotiation should succeed");
    send(&mut guest, &notify("hi", vec![]));
    send(
        &mut guest,
        &GuestMessage::ClipboardSet {
            text: "from guest".into(),
        },
    );
    send(&mut guest, &GuestMessage::ClipboardGet);
    match recv(&mut guest) {
        HostMessage::ClipboardData { text } => assert_eq!(text, "from guest"),
        other => panic!("expected ClipboardData, got {other:?}"),
    }
    drop(guest);
    server.join().unwrap().unwrap();

    // xdg-open is one request per connection: the host closes after it, so
    // the one-shot interceptor opens its own connection.
    let (_dir, path, server) = serve_one_guest(enabled_config(), Arc::clone(&rec));
    let mut guest = podbox_guest::socket::negotiate_at(&path, "dev", &[CAP_XDG_OPEN])
        .expect("negotiation should succeed");
    send(
        &mut guest,
        &GuestMessage::XdgOpen {
            uri: "https://example.com".into(),
        },
    );
    assert!(
        read_frame(&mut guest).unwrap().is_none(),
        "unexpected reply"
    );
    server.join().unwrap().unwrap();

    assert_eq!(
        rec.calls(),
        vec![
            "notify:hi:normal::0".to_string(),
            "clipboard_set".to_string(),
            "clipboard_get".to_string(),
            "open:https://example.com".to_string(),
        ]
    );
}

#[test]
fn guest_host_exec_succeeds_when_granted() {
    let (_dir, path, server) = serve_one_guest(enabled_config(), Arc::default());
    let mut guest = podbox_guest::socket::negotiate_at(&path, "dev", &[CAP_HOST_EXEC])
        .expect("negotiation should succeed");
    send(
        &mut guest,
        &GuestMessage::HostExec {
            cmd: "echo".into(),
            args: vec!["via guest".into()],
        },
    );
    match recv(&mut guest) {
        HostMessage::HostExecStdout { data } => assert!(data.contains("via guest"), "{data:?}"),
        other => panic!("expected HostExecStdout, got {other:?}"),
    }
    assert!(matches!(
        recv(&mut guest),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    drop(guest);
    server.join().unwrap().unwrap();
}

#[test]
fn guest_refused_capability_is_a_typed_error_and_nothing_runs() {
    let mut cfg = enabled_config();
    cfg.integration.notify = false;
    let rec = Arc::new(Recorder::default());
    let (_dir, path, server) = serve_one_guest(cfg, Arc::clone(&rec));

    let err = podbox_guest::socket::negotiate_at(&path, "dev", &[CAP_NOTIFY])
        .expect_err("a refused capability must not negotiate");
    assert!(
        matches!(err, podbox_guest::error::GuestError::CapabilityRefused(_)),
        "{err:?}"
    );
    server.join().unwrap().unwrap();
    assert!(rec.calls().is_empty());
}

#[test]
fn raw_request_without_hello_is_not_delivered() {
    // What the interceptors did before spec 001: connect and send the request
    // with no Hello. The host must refuse it.
    let rec = Arc::new(Recorder::default());
    let (_dir, path, server) = serve_one_guest(enabled_config(), Arc::clone(&rec));
    let mut raw = UnixStream::connect(&path).unwrap();
    raw.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    send(&mut raw, &notify("sneaky", vec![]));
    expect_error(recv(&mut raw), "not accepted");
    drop(raw);
    server.join().unwrap().unwrap();
    assert!(rec.calls().is_empty(), "{:?}", rec.calls());
}

// ── Streaming host-exec (new capability, old guests unaffected) ──

/// Collected streaming output: stdout/stderr chunks plus the exit.
type StreamedOutput = (Vec<(ExecStream, Vec<u8>)>, i32, Option<i32>);

/// Hello with the streaming capability and run one command, collecting
/// every frame until `HostExecDone`.
fn run_streamed(c: &mut UnixStream, cmd: &str, args: Vec<String>) -> StreamedOutput {
    send(
        c,
        &GuestMessage::HostExec {
            cmd: cmd.into(),
            args,
        },
    );
    let mut chunks = Vec::new();
    loop {
        match recv(c) {
            HostMessage::HostExecOutput { stream, data } => chunks.push((stream, data)),
            HostMessage::HostExecDone { exit_code, signal } => {
                return (chunks, exit_code, signal);
            }
            other => panic!("expected streamed output or done, got {other:?}"),
        }
    }
}

fn hello_streaming() -> GuestMessage {
    hello(&[CAP_HOST_EXEC, CAP_HOST_EXEC_STREAM])
}

#[test]
fn stream_capability_is_granted_alongside_host_exec() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello_streaming());
    match recv(&mut c) {
        HostMessage::HelloAck { accepted, .. } => {
            assert!(accepted.contains(&CAP_HOST_EXEC.to_string()));
            assert!(accepted.contains(&CAP_HOST_EXEC_STREAM.to_string()));
        }
        other => panic!("expected HelloAck, got {other:?}"),
    }
    finish(c, h).unwrap();
}

#[test]
fn stream_capability_is_rejected_when_host_exec_is_disabled() {
    let mut cfg = enabled_config();
    cfg.integration.host_exec.enabled = false;
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello_streaming());
    match recv(&mut c) {
        HostMessage::HelloAck { accepted, .. } => {
            assert!(!accepted.contains(&CAP_HOST_EXEC_STREAM.to_string()));
        }
        other => panic!("expected HelloAck, got {other:?}"),
    }
    finish(c, h).unwrap();
}

#[test]
fn streamed_binary_output_arrives_byte_identical() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello_streaming());
    let _ = recv(&mut c);
    // The shell binary's first KiB is not valid UTF-8: a lossy conversion
    // would corrupt these bytes.
    let expected = std::fs::read("/bin/sh").unwrap()[..1024].to_vec();
    let (chunks, code, signal) = run_streamed(
        &mut c,
        "sh",
        vec!["-c".into(), "head -c 1024 /bin/sh".into()],
    );
    let stdout: Vec<u8> = chunks
        .iter()
        .filter(|(s, _)| *s == ExecStream::Stdout)
        .flat_map(|(_, d)| d.clone())
        .collect();
    assert_eq!(stdout, expected);
    assert_eq!(code, 0);
    assert_eq!(signal, None);
    finish(c, h).unwrap();
}

#[test]
fn streamed_large_output_uses_many_frames_and_stays_complete() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello_streaming());
    let _ = recv(&mut c);
    let (chunks, code, _) = run_streamed(
        &mut c,
        "sh",
        vec!["-c".into(), "head -c 100000 /dev/urandom".into()],
    );
    let total: usize = chunks.iter().map(|(_, d)| d.len()).sum();
    assert_eq!(total, 100_000);
    assert!(
        chunks.len() > 1,
        "expected several chunks, got {}",
        chunks.len()
    );
    assert_eq!(code, 0);
    finish(c, h).unwrap();
}

#[test]
fn streamed_signal_death_reports_the_shell_convention_code() {
    // A script that kills itself: `$$` inside the file is not an argument,
    // so the host-exec argument filter does not apply to it.
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("selfterm.sh");
    std::fs::write(&script, "#!/bin/sh\nkill -TERM $$\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut cfg = enabled_config();
    cfg.integration
        .host_exec
        .allowlist
        .as_mut()
        .unwrap()
        .insert(
            "selfterm".to_string(),
            HostExecEntry::Simple(script.to_string_lossy().into_owned()),
        );
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello_streaming());
    let _ = recv(&mut c);
    let (_, code, signal) = run_streamed(&mut c, "selfterm", vec![]);
    assert_eq!(code, 143);
    assert_eq!(signal, Some(15));
    finish(c, h).unwrap();
}

#[test]
fn unstreamed_guest_still_gets_buffered_frames() {
    // A guest from before streaming (no `host_exec_stream` capability)
    // gets exactly the historical framing: one stdout frame, one done.
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello(&[CAP_HOST_EXEC]));
    let _ = recv(&mut c);
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "echo".into(),
            args: vec!["legacy".into()],
        },
    );
    match recv(&mut c) {
        HostMessage::HostExecStdout { data } => assert!(data.contains("legacy")),
        other => panic!("expected HostExecStdout, got {other:?}"),
    }
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    finish(c, h).unwrap();
}
