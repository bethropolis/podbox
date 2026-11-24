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

/// Host terminal variables that describe the invoking terminal.
///
/// These are presentation metadata, not host capabilities: they carry no
/// socket paths or credentials. Podbox forwards them to interactive
/// container sessions and host-exec children. Terminal control sockets
/// (`KITTY_LISTEN_ON`, `WEZTERM_UNIX_SOCKET`) and multiplexer state (`TMUX`,
/// `STY`) deliberately stay out.
pub const TERMINAL_ENV_KEYS: &[&str] = &[
    "TERM",
    "COLORTERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
    "VTE_VERSION",
    "KONSOLE_VERSION",
    "KITTY_WINDOW_ID",
    "WEZTERM_PANE",
    "WT_SESSION",
    "LC_TERMINAL",
    "LC_TERMINAL_VERSION",
];

/// Whether a guest may pass `key` through to a host-exec child.
///
/// The child runs on the host, so only presentation metadata and locale
/// cross the boundary; `PATH`, `LD_PRELOAD` and host socket variables do
/// not. Old hosts apply the narrower historical set and drop the rest,
/// which is why extending this list needs no protocol bump.
#[must_use]
pub fn is_host_exec_env(key: &str) -> bool {
    key == "LANG" || TERMINAL_ENV_KEYS.contains(&key)
}

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
