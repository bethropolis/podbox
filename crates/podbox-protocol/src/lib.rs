mod capabilities;
mod framing;
mod messages;

pub use capabilities::{
    ALL_CAPABILITIES, CAP_CLIPBOARD, CAP_HOST_EXEC, CAP_HOST_EXEC_FDS, CAP_HOST_EXEC_STREAM,
    CAP_NOTIFY, CAP_XDG_OPEN, INTERCEPTORS, TERMINAL_ENV_KEYS, is_host_exec_env,
};
pub use framing::{FrameRead, FrameReader, read_frame, write_frame};
pub use messages::{ExecStream, GuestMessage, HostMessage, NotifyAction};

/// Increment on breaking wire-format changes. Backwards-compatible
/// additions (new optional message types) do NOT increment this.
pub const PROTOCOL_VERSION: u32 = 1;

/// Largest clipboard payload either side will send or accept (8 MiB).
/// Beyond this the host answers with an `Error` frame and the guest
/// refuses locally with a clear message, instead of failing the frame.
/// A fixed constant (not a config key): raising it needs the matching
/// host/guest rollout, and the frame cap bounds it anyway.
pub const MAX_CLIPBOARD_BYTES: usize = 8 * 1024 * 1024;

/// Unix-only helpers shared by the host CLI and the guest daemon.
#[cfg(unix)]
pub mod sysutil;

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::io::{self, Read};

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
    fn frame_length_prefix_matches_payload() {
        let msg = GuestMessage::ClipboardGet;
        let mut buf = Vec::new();
        write_frame(&mut buf, &msg).unwrap();
        let len = u32::from_be_bytes(buf[..4].try_into().unwrap()) as usize;
        assert_eq!(len, buf[4..].len());
    }

    #[test]
    fn interceptors_cover_known_binaries_and_capabilities() {
        use std::collections::HashSet;
        let binaries: HashSet<&str> = INTERCEPTORS.iter().map(|(bin, _)| *bin).collect();
        assert_eq!(binaries.len(), INTERCEPTORS.len(), "duplicate binary");
        for (_, cap) in INTERCEPTORS {
            assert!(ALL_CAPABILITIES.contains(cap), "unknown capability {cap}");
        }
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

        let payload = read_frame(&mut &buf[..]).unwrap().unwrap();
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

        let payload = read_frame(&mut &buf[..]).unwrap().unwrap();
        let decoded: GuestMessage = serde_json::from_slice(&payload).unwrap();
        match decoded {
            GuestMessage::ClipboardSet { text } => {
                assert_eq!(text, "clipboard content");
            }
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn hello_ack_with_shims_round_trips() {
        let msg = HostMessage::HelloAck {
            accepted: vec!["host_exec".into()],
            rejected: vec![],
            idle_timeout_secs: 10,
            host_exec_shims: vec!["git".into(), "code".into()],
            host_version: "0.9.5".into(),
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &msg).unwrap();
        let payload = read_frame(&mut &buf[..]).unwrap().unwrap();
        let decoded: HostMessage = serde_json::from_slice(&payload).unwrap();
        match decoded {
            HostMessage::HelloAck {
                accepted,
                host_exec_shims,
                host_version,
                ..
            } => {
                assert_eq!(accepted, vec!["host_exec"]);
                assert_eq!(host_exec_shims, vec!["git", "code"]);
                assert_eq!(host_version, "0.9.5");
            }
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn hello_ack_backward_compat_missing_shims() {
        let json = r#"{"type":"hello_ack","accepted":[],"rejected":[],"idle_timeout_secs":0}"#;
        let msg: HostMessage = serde_json::from_slice(json.as_bytes()).unwrap();
        match msg {
            HostMessage::HelloAck {
                host_exec_shims,
                host_version,
                ..
            } => {
                assert!(host_exec_shims.is_empty());
                assert!(host_version.is_empty());
            }
            _ => panic!("wrong message type"),
        }
    }

    /// One scripted step of a fake socket: bytes, or a read timeout.
    enum Step {
        Bytes(Vec<u8>),
        Timeout,
    }

    struct Scripted(std::collections::VecDeque<Step>);

    impl Read for Scripted {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match self.0.pop_front() {
                None => Ok(0),
                Some(Step::Timeout) => Err(io::Error::from(io::ErrorKind::WouldBlock)),
                Some(Step::Bytes(mut b)) => {
                    let n = b.len().min(buf.len());
                    buf[..n].copy_from_slice(&b[..n]);
                    if n < b.len() {
                        b.drain(..n);
                        self.0.push_front(Step::Bytes(b));
                    }
                    Ok(n)
                }
            }
        }
    }

    fn encoded(msg: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        write_frame(&mut buf, &msg).unwrap();
        buf
    }

    /// A timeout after the first two length bytes must not lose them: the
    /// frame still comes out whole, with no misparse of the payload.
    #[test]
    fn frame_survives_a_timeout_in_the_length_prefix() {
        let bytes = encoded("hello there");
        let mut r = Scripted(
            [
                Step::Bytes(bytes[..2].to_vec()),
                Step::Timeout,
                Step::Bytes(bytes[2..].to_vec()),
            ]
            .into(),
        );
        let mut fr = FrameReader::new();
        assert!(fr.read(&mut r).is_err());
        assert!(fr.in_progress());
        let got = fr.read(&mut r).unwrap();
        assert_eq!(
            got,
            FrameRead::Frame(serde_json::to_vec("hello there").unwrap())
        );
        assert!(!fr.in_progress());
    }

    #[test]
    fn frame_survives_a_timeout_in_the_payload() {
        let bytes = encoded("a longer payload to split");
        let cut = 4 + 7;
        let mut r = Scripted(
            [
                Step::Bytes(bytes[..cut].to_vec()),
                Step::Timeout,
                Step::Bytes(bytes[cut..].to_vec()),
            ]
            .into(),
        );
        let mut fr = FrameReader::new();
        assert!(fr.read(&mut r).is_err());
        assert_eq!(
            fr.read(&mut r).unwrap(),
            FrameRead::Frame(serde_json::to_vec("a longer payload to split").unwrap())
        );
    }

    #[test]
    fn clean_eof_between_frames_is_eof_not_an_error() {
        let mut r = Scripted(std::collections::VecDeque::new());
        assert_eq!(FrameReader::new().read(&mut r).unwrap(), FrameRead::Eof);
    }

    #[test]
    fn eof_inside_a_frame_is_an_error() {
        let bytes = encoded("cut short");
        let mut r = Scripted([Step::Bytes(bytes[..6].to_vec())].into());
        let err = FrameReader::new().read(&mut r).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn host_exec_output_chunk_round_trips_raw_bytes() {
        // Invalid UTF-8 must survive the frame exactly: chunks carry bytes,
        // not lossy strings.
        let msg = HostMessage::HostExecOutput {
            stream: ExecStream::Stdout,
            data: vec![0xff, 0xfe, b'A'],
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &msg).unwrap();
        let payload = read_frame(&mut &buf[..]).unwrap().unwrap();
        let decoded: HostMessage = serde_json::from_slice(&payload).unwrap();
        match decoded {
            HostMessage::HostExecOutput { stream, data } => {
                assert_eq!(stream, ExecStream::Stdout);
                assert_eq!(data, vec![0xff, 0xfe, b'A']);
            }
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn host_exec_v2_optional_fields_are_serde_defaulted() {
        // A minimal V2 frame (old guest shims that only know cmd/args,
        // mostly future guests) must decode with every new field defaulted:
        // the wire format is frozen without bumping PROTOCOL_VERSION.
        let json = br#"{"type":"host_exec_v2","cmd":"cat","args":["-s"]}"#;
        let msg: GuestMessage = serde_json::from_slice(json).unwrap();
        match msg {
            GuestMessage::HostExecV2 {
                cmd,
                args,
                env,
                cwd,
                tty,
            } => {
                assert_eq!(cmd, "cat");
                assert_eq!(args, vec!["-s"]);
                assert!(env.is_empty());
                assert!(cwd.is_none());
                assert!(!tty);
            }
            other => panic!("expected HostExecV2, got {other:?}"),
        }
    }

    #[test]
    fn host_exec_v2_round_trips_the_full_field_set() {
        let msg = GuestMessage::HostExecV2 {
            cmd: "git".into(),
            args: vec!["log".into()],
            env: vec![("TERM".into(), "xterm-256color".into())],
            cwd: Some("/home/user/project".into()),
            tty: true,
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &msg).unwrap();
        let payload = read_frame(&mut &buf[..]).unwrap().unwrap();
        let decoded: GuestMessage = serde_json::from_slice(&payload).unwrap();
        match decoded {
            GuestMessage::HostExecV2 {
                cmd,
                args,
                env,
                cwd,
                tty,
            } => {
                assert_eq!(cmd, "git");
                assert_eq!(args, vec!["log"]);
                assert_eq!(
                    env,
                    vec![("TERM".to_string(), "xterm-256color".to_string())]
                );
                assert_eq!(cwd.as_deref(), Some("/home/user/project"));
                assert!(tty);
            }
            other => panic!("expected HostExecV2, got {other:?}"),
        }
    }

    #[test]
    fn terminal_env_allowlist_carries_identity_but_not_host_sockets() {
        for key in [
            "TERM",
            "COLORTERM",
            "TERM_PROGRAM",
            "TERM_PROGRAM_VERSION",
            "VTE_VERSION",
            "KITTY_WINDOW_ID",
            "LC_TERMINAL",
        ] {
            assert!(TERMINAL_ENV_KEYS.contains(&key), "{key} must be forwarded");
            assert!(is_host_exec_env(key), "{key} must reach host-exec");
        }
        assert!(is_host_exec_env("LANG"));
        for key in [
            "PATH",
            "LD_PRELOAD",
            "KITTY_LISTEN_ON",
            "WEZTERM_UNIX_SOCKET",
            "TMUX",
            "STY",
        ] {
            assert!(!is_host_exec_env(key), "{key} must stay inside");
            assert!(
                !TERMINAL_ENV_KEYS.contains(&key),
                "{key} is not terminal identity"
            );
        }
    }

    #[test]
    fn host_exec_signal_round_trips() {
        let msg = GuestMessage::HostExecSignal { sig: 2 };
        let mut buf = Vec::new();
        write_frame(&mut buf, &msg).unwrap();
        let payload = read_frame(&mut &buf[..]).unwrap().unwrap();
        let decoded: GuestMessage = serde_json::from_slice(&payload).unwrap();
        match decoded {
            GuestMessage::HostExecSignal { sig } => assert_eq!(sig, 2),
            other => panic!("expected HostExecSignal, got {other:?}"),
        }
    }

    /// Wire compatibility in the other direction: a host built before
    /// `host_exec_fds` can still parse a `Hello` that requests it — the
    /// unknown capability lands in `capabilities` and negotiation rejects
    /// it, which is how old hosts degrade the guest to plain `HostExec`.
    #[test]
    fn hello_with_an_unknown_capability_parses_on_the_old_schema() {
        // Serde deserializes every declared field, so the enum's own test
        // reads are the negotiate path, not these bindings.
        #[allow(dead_code)]
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum OldGuestMessage {
            Hello {
                protocol_version: u32,
                guest_version: String,
                container: String,
                capabilities: Vec<String>,
            },
        }

        let json = r#"{"type":"hello","protocol_version":1,"guest_version":"0.2.0",
            "container":"dev","capabilities":["host_exec","host_exec_fds"]}"#;
        let msg: OldGuestMessage = serde_json::from_slice(json.as_bytes()).unwrap();
        let OldGuestMessage::Hello { capabilities, .. } = msg;
        assert_eq!(capabilities, vec!["host_exec", "host_exec_fds"]);
    }

    #[test]
    fn host_exec_done_carries_an_optional_signal() {
        let msg = HostMessage::HostExecDone {
            exit_code: 143,
            signal: Some(15),
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &msg).unwrap();
        let payload = read_frame(&mut &buf[..]).unwrap().unwrap();
        let decoded: HostMessage = serde_json::from_slice(&payload).unwrap();
        match decoded {
            HostMessage::HostExecDone { exit_code, signal } => {
                assert_eq!(exit_code, 143);
                assert_eq!(signal, Some(15));
            }
            _ => panic!("wrong message type"),
        }
    }

    /// Wire compatibility: a guest built before `signal` existed must
    /// still parse the new `HostExecDone`. Serde ignores the unknown
    /// field, so old guests keep working with no version bump.
    #[test]
    fn host_exec_done_with_signal_parses_under_the_old_schema() {
        #[derive(Debug, Deserialize, PartialEq, Eq)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum OldHostMessage {
            HostExecDone { exit_code: i32 },
        }

        let json = r#"{"type":"host_exec_done","exit_code":143,"signal":15}"#;
        let msg: OldHostMessage = serde_json::from_slice(json.as_bytes()).unwrap();
        assert_eq!(msg, OldHostMessage::HostExecDone { exit_code: 143 });
    }
}
