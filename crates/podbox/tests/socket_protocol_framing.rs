//! Frame validation, hello deadlines, and session error recovery tests.

#[path = "support/socket_protocol.rs"]
mod socket_protocol;

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

use podbox::protocol::{CAP_NOTIFY, HostMessage, write_frame};
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

/// Write a length-prefixed frame by hand, so a test can send bad framing.
fn write_raw(c: &mut UnixStream, len: u32, payload: &[u8]) {
    c.write_all(&len.to_be_bytes()).unwrap();
    c.write_all(payload).unwrap();
    c.flush().unwrap();
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
