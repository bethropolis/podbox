//! Handshake, capability negotiation, and real-guest acceptance tests.

#[path = "support/socket_protocol.rs"]
mod socket_protocol;

use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use podbox::config::Config;
use podbox::protocol::{
    CAP_CLIPBOARD, CAP_HOST_EXEC, CAP_NOTIFY, CAP_XDG_OPEN, GuestMessage, HostMessage,
    NotifyAction, PROTOCOL_VERSION, read_frame,
};
use podbox::socket_host::{CwdMap, serve_connection};
use socket_protocol::{Recorder, enabled_config, finish, hello, recv, send, start};

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

fn notify(summary: &str, actions: Vec<NotifyAction>) -> GuestMessage {
    GuestMessage::Notify {
        summary: summary.into(),
        body: "body".into(),
        urgency: "normal".into(),
        actions,
        app_name: String::new(),
    }
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
) -> (tempfile::TempDir, PathBuf, JoinHandle<anyhow::Result<()>>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dev.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let cwd_map = CwdMap::for_daemon(&cfg);
        serve_connection(&mut stream, &cfg, "dev", &*rec, &cwd_map)
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
