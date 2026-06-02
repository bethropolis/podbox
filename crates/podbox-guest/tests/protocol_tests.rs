use std::io::Cursor;

use podbox_guest::protocol::{read_frame, write_frame, GuestMessage, HostMessage};

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
        } => {
            assert_eq!(summary, "hello");
            assert_eq!(body, "world");
            assert_eq!(urgency, "normal");
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
