//! Desktop notification, URI-opening, and clipboard integration tests.

#[path = "support/socket_protocol.rs"]
mod socket_protocol;

use std::sync::Arc;

use podbox::protocol::{
    CAP_CLIPBOARD, CAP_NOTIFY, CAP_XDG_OPEN, GuestMessage, HostMessage, NotifyAction, read_frame,
};
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
