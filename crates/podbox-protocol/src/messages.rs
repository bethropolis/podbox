use serde::{Deserialize, Serialize};

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
        /// The host applies only [`crate::is_host_exec_env`] keys.
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
        /// Version of the live host speaking this handshake, so the guest can
        /// compare it with the version baked into the image.
        #[serde(default)]
        host_version: String,
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
