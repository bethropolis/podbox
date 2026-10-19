use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

/// Increment on breaking wire-format changes. Backwards-compatible
/// additions (new optional message types) do NOT increment this.
pub const PROTOCOL_VERSION: u32 = 1;

/// Guest protocol capability identifiers.
///
/// Single source of truth — always use these constants in match arms,
/// construction, and capability negotiation rather than inline strings.
pub const CAP_NOTIFY: &str = "notify";
pub const CAP_XDG_OPEN: &str = "xdg_open";
pub const CAP_CLIPBOARD: &str = "clipboard";
pub const CAP_HOST_EXEC: &str = "host_exec";
/// Streaming host-exec output (`HostExecOutput` byte chunks instead of one
/// buffered `HostExecStdout`/`HostExecStderr` frame). Requested alongside
/// `host_exec`; old hosts leave it in the rejected list and the guest falls
/// back to buffered frames.
pub const CAP_HOST_EXEC_STREAM: &str = "host_exec_stream";
/// Host-exec with the child's stdio supplied by the guest: a
/// `HostExecV2` frame with three descriptors attached via `SCM_RIGHTS`,
/// plus `HostExecSignal` relay while the child runs. Old hosts leave it
/// in the rejected list and the guest falls back to plain `HostExec`.
pub const CAP_HOST_EXEC_FDS: &str = "host_exec_fds";

/// All known capabilities in negotiation order.
pub const ALL_CAPABILITIES: &[&str] = &[
    CAP_NOTIFY,
    CAP_XDG_OPEN,
    CAP_CLIPBOARD,
    CAP_HOST_EXEC,
    CAP_HOST_EXEC_STREAM,
    CAP_HOST_EXEC_FDS,
];

/// Interceptor binaries and the capability each needs: `(binary, capability)`.
///
/// Single source of truth for shim installation and the core-interceptor
/// guard in the guest daemon. (`podmgr-*` names are legacy aliases handled
/// separately at the use sites.)
pub const INTERCEPTORS: &[(&str, &str)] = &[
    ("notify-send", CAP_NOTIFY),
    ("xdg-open", CAP_XDG_OPEN),
    ("podbox-clipboard", CAP_CLIPBOARD),
    ("host-exec", CAP_HOST_EXEC),
];

/// Messages sent from guest to host.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GuestMessage {
    Hello {
        protocol_version: u32,
        guest_version: String,
        container: String,
        capabilities: Vec<String>,
    },
    Notify {
        summary: String,
        body: String,
        urgency: String,
        #[serde(default)]
        actions: Vec<NotifyAction>,
        #[serde(default)]
        app_name: String,
    },
    XdgOpen {
        uri: String,
    },
    ClipboardSet {
        text: String,
    },
    ClipboardGet,
    HostExec {
        cmd: String,
        args: Vec<String>,
    },
    /// Host-exec with the child's stdio taken from descriptors the guest
    /// attaches to this frame (capability `host_exec_fds`).
    ///
    /// Exactly three descriptors ride via `SCM_RIGHTS` on the same message,
    /// in stdin, stdout, stderr order. The host uses them directly as the
    /// child's stdio, so binary data and large outputs never pass through
    /// the protocol. A guest whose request is refused by this capability
    /// keeps using [`GuestMessage::HostExec`] instead.
    ///
    /// The optional fields are serde-defaulted so the wire format here is
    /// frozen now, while the host phases in behaviour behind them:
    /// `cmd`/`args`/`env` are consumed, `cwd` is carried but not yet
    /// processed, and `tty` must be false (the host refuses `true` with a
    /// typed error until the TTY path exists).
    HostExecV2 {
        cmd: String,
        args: Vec<String>,
        /// Key/value pairs the guest asks the host to export to the child.
        /// The host applies only allowlisted keys (TERM, LANG, COLORTERM).
        #[serde(default)]
        env: Vec<(String, String)>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        tty: bool,
    },
    /// Relay a signal (SIGINT, SIGTERM, SIGHUP or SIGWINCH) from the guest
    /// shim to a running host-exec child. Only sent on a `host_exec_fds`
    /// session: the host forwards it to the child's process group. The
    /// child's death is reported through the regular `HostExecDone` frame.
    HostExecSignal {
        sig: i32,
    },
    /// Sent by the host CLI to register a new terminal session.
    /// The `pidfd` follows via `SCM_RIGHTS` on the same connection.
    RegisterSession,
    /// Sent by the guest daemon when user processes are still running.
    Busy,
    /// Sent by the guest daemon when no user processes remain.
    IdleTimeout,
    /// Guest reply to host `GetInfo`.
    Info {
        guest_version: String,
        protocol_version: u32,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NotifyAction {
    pub key: String,
    pub label: String,
}

/// Which pipe of a streaming host-exec child a chunk came from.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecStream {
    Stdout,
    Stderr,
}

/// Messages sent from host to guest.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostMessage {
    HelloAck {
        accepted: Vec<String>,
        rejected: Vec<String>,
        #[serde(default)]
        idle_timeout_secs: u64,
        /// List of command names for which the guest daemon should create PATH shims.
        #[serde(default)]
        host_exec_shims: Vec<String>,
    },
    ClipboardData {
        text: String,
    },
    HostExecStdout {
        data: String,
    },
    HostExecStderr {
        data: String,
    },
    /// One chunk of a streaming command's output, sent as the child
    /// produces it. `data` is raw bytes (up to 16 KiB per chunk) so binary
    /// output survives intact. Only sent when the guest negotiated
    /// `host_exec_stream`; older guests keep receiving the buffered
    /// `HostExecStdout`/`HostExecStderr` frames.
    HostExecOutput {
        stream: ExecStream,
        data: Vec<u8>,
    },
    HostExecDone {
        exit_code: i32,
        /// Signal that killed the child, if any. `exit_code` already
        /// follows the shell convention (`128 + signal`); this names the
        /// cause. Old guests ignore the unknown field.
        #[serde(default)]
        signal: Option<i32>,
    },
    NotifyActionResult {
        notification_id: u32,
        action_key: String,
    },
    Ping,
    Shutdown,
    /// Sent by the host when all CLI sessions have ended; guest responds
    /// with `IdleTimeout` or `Busy` after scanning `/proc`.
    CheckIdle,
    /// Sent by the host when a guest message is rejected (missing `Hello`
    /// negotiation, or a capability that was not accepted).
    Error {
        reason: String,
        /// Optional machine-readable code (e.g. `busy`) so clients can
        /// distinguish rejection kinds without parsing `reason`.
        #[serde(default)]
        code: Option<String>,
    },
    /// Host → guest version/info query (doctor).
    GetInfo,
}

/// Write a length-prefixed JSON frame.
pub fn write_frame<W: Write>(w: &mut W, msg: &impl Serialize) -> io::Result<()> {
    let json = serde_json::to_vec(msg)?;
    let len = u32::try_from(json.len())
        .expect("frame payload exceeds 4 GiB")
        .to_be_bytes();
    w.write_all(&len)?;
    w.write_all(&json)?;
    w.flush()?;
    Ok(())
}

/// Maximum frame size: 16 MiB.
const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// Largest clipboard payload either side will send or accept (8 MiB).
/// Beyond this the host answers with an `Error` frame and the guest
/// refuses locally with a clear message, instead of failing the frame.
/// A fixed constant (not a config key): raising it needs the matching
/// host/guest rollout, and the frame cap bounds it anyway.
pub const MAX_CLIPBOARD_BYTES: usize = 8 * 1024 * 1024;

/// Read a length-prefixed JSON frame.
pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame too large: {len} bytes (max {MAX_FRAME_SIZE})"),
        ));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
}

/// Incremental frame reader that survives read timeouts.
///
/// [`read_frame`] uses `read_exact`, which discards bytes it already read when
/// a timeout fires mid-frame. The stream is then misaligned: the next read
/// parses payload bytes as a length prefix. `FrameReader` keeps the partial
/// frame, so a timeout returns `WouldBlock`/`TimedOut` with nothing lost and
/// the caller can retry, or give up once [`FrameReader::stalled_for`] is long.
#[derive(Debug, Default)]
pub struct FrameReader {
    prefix: [u8; 4],
    prefix_filled: usize,
    body: Vec<u8>,
    body_filled: usize,
    body_len: Option<usize>,
    /// When the first byte of the frame in progress arrived.
    started: Option<std::time::Instant>,
}

/// What a [`FrameReader::read`] call produced.
#[derive(Debug, PartialEq, Eq)]
pub enum FrameRead {
    /// A complete frame payload.
    Frame(Vec<u8>),
    /// The peer closed the stream at a frame boundary.
    Eof,
}

impl FrameReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when part of a frame has been read and the rest is still due.
    pub fn in_progress(&self) -> bool {
        self.started.is_some()
    }

    /// Time since the first byte of the frame in progress, if one is in progress.
    pub fn stalled_for(&self) -> Option<std::time::Duration> {
        self.started.map(|t| t.elapsed())
    }

    /// Read until one frame is complete. A timeout returns `Err` and keeps the
    /// partial frame, so a later call resumes exactly where this one stopped.
    pub fn read<R: Read>(&mut self, r: &mut R) -> io::Result<FrameRead> {
        while self.prefix_filled < 4 {
            let n = r.read(&mut self.prefix[self.prefix_filled..])?;
            if n == 0 {
                return if self.prefix_filled == 0 {
                    Ok(FrameRead::Eof)
                } else {
                    Err(io::Error::from(io::ErrorKind::UnexpectedEof))
                };
            }
            self.started.get_or_insert_with(std::time::Instant::now);
            self.prefix_filled += n;
        }
        let len = match self.body_len {
            Some(len) => len,
            None => {
                let len = u32::from_be_bytes(self.prefix) as usize;
                if len > MAX_FRAME_SIZE {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("frame too large: {len} bytes (max {MAX_FRAME_SIZE})"),
                    ));
                }
                self.body = vec![0u8; len];
                self.body_len = Some(len);
                len
            }
        };
        while self.body_filled < len {
            let n = r.read(&mut self.body[self.body_filled..])?;
            if n == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            self.body_filled += n;
        }
        let frame = std::mem::take(&mut self.body);
        *self = Self::default();
        Ok(FrameRead::Frame(frame))
    }
}

/// Unix-only helpers shared by the host CLI and the guest daemon.
#[cfg(unix)]
pub mod sysutil;

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &msg).unwrap();
        let payload = read_frame(&mut &buf[..]).unwrap().unwrap();
        let decoded: HostMessage = serde_json::from_slice(&payload).unwrap();
        match decoded {
            HostMessage::HelloAck {
                accepted,
                host_exec_shims,
                ..
            } => {
                assert_eq!(accepted, vec!["host_exec"]);
                assert_eq!(host_exec_shims, vec!["git", "code"]);
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
                host_exec_shims, ..
            } => assert!(host_exec_shims.is_empty()),
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
