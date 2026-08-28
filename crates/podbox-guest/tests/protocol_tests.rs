use std::io::Cursor;

use podbox_guest::protocol::{GuestMessage, HostMessage, read_frame, write_frame};

#[test]
fn hello_serializes_with_type_tag() {
    let msg = GuestMessage::Hello {
        protocol_version: 1,
        guest_version: "0.2.0".into(),
        container: "myenv".into(),
        capabilities: vec!["notify".into()],
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"hello\""));
}

#[test]
fn hello_ack_serializes_with_type_tag() {
    let msg = HostMessage::HelloAck {
        accepted: vec!["notify".into()],
        rejected: vec![],
        idle_timeout_secs: 0,
        host_exec_shims: vec![],
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"hello_ack\""));
}

#[test]
fn frame_length_prefix_matches_payload() {
    let msg = GuestMessage::ClipboardGet;
    let mut buf = Vec::new();
    write_frame(&mut buf, &msg).unwrap();
    let len = u32::from_be_bytes(buf[..4].try_into().unwrap()) as usize;
    assert_eq!(len, buf[4..].len());
}

#[test]
fn roundtrip_notify_message() {
    let msg = GuestMessage::Notify {
        summary: "hello".into(),
        body: "world".into(),
        urgency: "normal".into(),
        actions: vec![],
        app_name: String::new(),
    };
    let mut buf = Vec::new();
    write_frame(&mut buf, &msg).unwrap();

    let payload = read_frame(&mut Cursor::new(&buf)).unwrap().unwrap();
    let decoded: GuestMessage = serde_json::from_slice(&payload).unwrap();
    match decoded {
        GuestMessage::Notify {
            summary,
            body,
            urgency,
            actions,
            app_name: _,
        } => {
            assert_eq!(summary, "hello");
            assert_eq!(body, "world");
            assert_eq!(urgency, "normal");
            assert!(actions.is_empty());
        }
        _ => panic!("wrong message type"),
    }
}

#[test]
fn roundtrip_clipboard_set() {
    let msg = GuestMessage::ClipboardSet {
        text: "clipboard content".into(),
    };
    let mut buf = Vec::new();
    write_frame(&mut buf, &msg).unwrap();

    let payload = read_frame(&mut Cursor::new(&buf)).unwrap().unwrap();
    let decoded: GuestMessage = serde_json::from_slice(&payload).unwrap();
    match decoded {
        GuestMessage::ClipboardSet { text } => {
            assert_eq!(text, "clipboard content");
        }
        _ => panic!("wrong message type"),
    }
}

#[test]
fn roundtrip_shutdown_message() {
    let msg = HostMessage::Shutdown;
    let mut buf = Vec::new();
    write_frame(&mut buf, &msg).unwrap();

    let payload = read_frame(&mut Cursor::new(&buf)).unwrap().unwrap();
    let decoded: HostMessage = serde_json::from_slice(&payload).unwrap();
    match decoded {
        HostMessage::Shutdown => {}
        _ => panic!("wrong message type"),
    }
}

#[test]
fn frame_eof_returns_none() {
    let empty: &[u8] = &[];
    let result = read_frame(&mut Cursor::new(empty)).unwrap();
    assert!(result.is_none());
}

#[test]
fn hello_ack_with_shims_round_trips() {
    let msg = HostMessage::HelloAck {
        accepted: vec!["notify".into(), "host_exec".into()],
        rejected: vec![],
        idle_timeout_secs: 30,
        host_exec_shims: vec!["git".into(), "flatpak".into()],
    };
    let mut buf = Vec::new();
    write_frame(&mut buf, &msg).unwrap();
    let payload = read_frame(&mut Cursor::new(&buf)).unwrap().unwrap();
    let decoded: HostMessage = serde_json::from_slice(&payload).unwrap();
    match decoded {
        HostMessage::HelloAck {
            accepted,
            rejected,
            idle_timeout_secs,
            host_exec_shims,
        } => {
            assert_eq!(accepted, vec!["notify", "host_exec"]);
            assert!(rejected.is_empty());
            assert_eq!(idle_timeout_secs, 30);
            assert_eq!(host_exec_shims, vec!["git", "flatpak"]);
        }
        _ => panic!("wrong message type"),
    }
}

#[test]
fn hello_ack_default_host_exec_shims_backward_compat() {
    // Old host without host_exec_shims field should deserialize as empty vec.
    let json = r#"{"type":"hello_ack","accepted":[],"rejected":[],"idle_timeout_secs":0}"#;
    let msg: HostMessage = serde_json::from_slice(json.as_bytes()).unwrap();
    match msg {
        HostMessage::HelloAck {
            host_exec_shims, ..
        } => {
            assert!(host_exec_shims.is_empty());
        }
        _ => panic!("wrong message type"),
    }
}
