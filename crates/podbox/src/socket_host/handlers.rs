//! Per-message protocol handlers for the host socket server (`hello`,
//! `host-exec`, `notify`, `xdg-open`, `clipboard`). Single cohesive concern;
//! stays above ~300 LOC because the message-dispatch contract is one unit
//! (documented exemption, per the modularization guide 1/8).
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::Duration;

use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::Pid;

use super::PING_INTERVAL;
use super::actions::HostActions;
use super::cwd_map::CwdMap;
use crate::config::{Config, HostExecCwd, IntegrationConfig};
use crate::process::FdCapturingReader;
use crate::protocol::{ExecStream, FrameRead, FrameReader, GuestMessage, HostMessage, write_frame};

/// How long a legacy (non fd-passing) host-exec child may run before it
/// is killed. The fd-passing session instead honours the allowlist
/// entry's own `timeout`, configured per command.
const HOST_EXEC_TIMEOUT: Duration = Duration::from_mins(1);

/// Total output (stdout + stderr) a streaming host-exec child may produce
/// before it is killed. Bounds host memory and keeps every frame far under
/// the frame cap. Per-entry configuration would need a config key.
const MAX_HOST_EXEC_OUTPUT: u64 = 64 * 1024 * 1024;

/// One read from a streaming child's pipe. Small enough that the JSON
/// encoding of a chunk stays far under the frame cap.
const EXEC_CHUNK_LEN: usize = 16 * 1024;

/// Outcome of a `Hello` handshake.
pub(super) enum HelloOutcome {
    /// Handshake accepted; carries the list of granted capabilities.
    Accepted(Vec<String>),
    /// Handshake rejected (e.g. protocol version mismatch).
    Rejected,
}

/// Handle a `Hello` handshake from the guest.
///
/// On success returns the list of accepted capabilities, which the connection
/// uses to gate subsequent messages. A protocol version mismatch is a failed
/// negotiation and must not grant any capability or claim the daemon stream.
pub(super) fn handle_hello(
    stream: &mut UnixStream,
    config: &IntegrationConfig,
    idle_timeout_secs: u64,
    protocol_version: u32,
    guest_version: String,
    container: String,
    capabilities: Vec<String>,
) -> anyhow::Result<HelloOutcome> {
    if protocol_version != crate::protocol::PROTOCOL_VERSION {
        tracing::error!(
            "protocol mismatch — got v{}, expected v{}",
            protocol_version,
            crate::protocol::PROTOCOL_VERSION
        );
        write_frame(stream, &HostMessage::Shutdown)?;
        return Ok(HelloOutcome::Rejected);
    }
    tracing::info!(
        "guest hello (v{}, container: {}, caps: {:?})",
        guest_version,
        container,
        capabilities
    );
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    for cap in capabilities {
        let enabled = match cap.as_str() {
            crate::protocol::CAP_NOTIFY => config.notify,
            crate::protocol::CAP_XDG_OPEN => config.xdg_open.enabled,
            crate::protocol::CAP_CLIPBOARD => config.clipboard,
            crate::protocol::CAP_HOST_EXEC => config.host_exec.enabled,
            // Streaming output still runs allowlisted commands only, so it
            // rides on the same switch as plain host-exec.
            crate::protocol::CAP_HOST_EXEC_STREAM => config.host_exec.enabled,
            // fd-passing is likewise allowlist-only: it adds stdio plumbing
            // to the same command gate, with the child in its own process
            // group and the guest env filtered to a fixed key list.
            crate::protocol::CAP_HOST_EXEC_FDS => config.host_exec.enabled,
            _ => false,
        };
        if enabled {
            accepted.push(cap);
        } else {
            rejected.push(cap);
        }
    }
    let host_exec_shims = if config.host_exec.enabled {
        config.host_exec.guest_shims()
    } else {
        Vec::new()
    };
    let response = HostMessage::HelloAck {
        accepted: accepted.clone(),
        rejected,
        idle_timeout_secs,
        host_exec_shims,
    };
    write_frame(stream, &response)?;
    Ok(HelloOutcome::Accepted(accepted))
}

/// Handle a `Notify` message from the guest.
///
/// Without actions the notification is fire-and-forget. With actions the
/// chosen key (or an empty key when the user picked none, or the notification
/// could not be shown) goes back to the guest.
pub(super) fn handle_notify(
    stream: &mut UnixStream,
    actions: &dyn HostActions,
    summary: String,
    body: String,
    urgency: String,
    app_name: String,
    choices: Vec<crate::protocol::NotifyAction>,
) -> anyhow::Result<()> {
    if choices.is_empty() {
        if let Err(e) = actions.notify(&summary, &body, &urgency, &app_name, &choices) {
            tracing::debug!("notification not shown: {e:#}");
        }
        return Ok(());
    }
    let action_key = actions
        .notify(&summary, &body, &urgency, &app_name, &choices)
        .unwrap_or_else(|e| {
            tracing::warn!("notification with actions not shown: {e:#}");
            String::new()
        });
    write_frame(
        stream,
        &HostMessage::NotifyActionResult {
            notification_id: 0,
            action_key,
        },
    )?;
    Ok(())
}

/// Longest URI accepted from the guest.
pub(crate) const MAX_URI_LEN: usize = 8192;

/// Why a guest-supplied URI was refused.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UriError {
    Empty,
    TooLong,
    ControlCharacter,
    Malformed,
    /// `scheme` is refused because it is not in `allowed`.
    SchemeNotAllowed {
        scheme: String,
        allowed: Vec<String>,
    },
    MissingHost,
}

impl std::fmt::Display for UriError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty URI"),
            Self::TooLong => write!(f, "URI longer than {MAX_URI_LEN} bytes"),
            Self::ControlCharacter => write!(f, "URI contains a control character"),
            Self::Malformed => write!(f, "not a URI"),
            Self::SchemeNotAllowed { scheme, allowed } => write!(
                f,
                "scheme '{scheme}' is not allowed (allowed: {})",
                allowed.join(", ")
            ),
            Self::MissingHost => write!(f, "web URL has no host"),
        }
    }
}

/// Check a guest-supplied URI and return the text to open.
///
/// A bare host such as `example.com` is opened over https. Paths and anything
/// that is not a URI are refused.
pub(crate) fn validate_uri(uri: &str, allowed: &[String]) -> Result<String, UriError> {
    let s = uri.trim();
    if s.is_empty() {
        return Err(UriError::Empty);
    }
    if s.len() > MAX_URI_LEN {
        return Err(UriError::TooLong);
    }
    if s.chars().any(char::is_control) {
        return Err(UriError::ControlCharacter);
    }
    if s.starts_with('/') || s.starts_with('.') {
        return Err(UriError::Malformed);
    }

    let (text, parsed) = match url::Url::parse(s) {
        Ok(parsed) => (s.to_string(), parsed),
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let wrapped = format!("https://{s}");
            let parsed = url::Url::parse(&wrapped).map_err(|_| UriError::Malformed)?;
            (wrapped, parsed)
        }
        Err(_) => return Err(UriError::Malformed),
    };

    let scheme = parsed.scheme().to_ascii_lowercase();
    if !allowed.iter().any(|a| a.eq_ignore_ascii_case(&scheme)) {
        return Err(UriError::SchemeNotAllowed {
            scheme,
            allowed: allowed.to_vec(),
        });
    }
    let web = scheme == "http" || scheme == "https";
    if web && parsed.host_str().is_none_or(str::is_empty) {
        return Err(UriError::MissingHost);
    }
    Ok(text)
}

/// Handle an `XdgOpen` message from the guest.
///
/// A refused URI gets an `Error` frame with the reason. Accepted or refused,
/// the caller closes the connection afterwards: the guest's `xdg-open` waits
/// for that close, and treats an `Error` frame as failure.
pub(super) fn handle_xdg_open(
    stream: &mut UnixStream,
    actions: &dyn HostActions,
    allowed: &[String],
    uri: String,
) -> anyhow::Result<()> {
    match validate_uri(&uri, allowed) {
        Ok(validated) => actions.open_uri(&validated)?,
        Err(e) => {
            tracing::warn!("refused xdg-open request: {e}");
            write_frame(
                stream,
                &HostMessage::Error {
                    reason: format!("xdg-open refused: {e}"),
                    code: None,
                },
            )?;
        }
    }
    Ok(())
}

/// Handle a `ClipboardSet` message from the guest.
///
/// Oversized content gets an `Error` reply and the connection stays open:
/// dropping it would punish a client that can simply send less.
pub(super) fn handle_clipboard_set(
    stream: &mut UnixStream,
    actions: &dyn HostActions,
    text: String,
) -> anyhow::Result<()> {
    if text.len() > crate::protocol::MAX_CLIPBOARD_BYTES {
        write_frame(
            stream,
            &HostMessage::Error {
                reason: format!(
                    "clipboard content exceeds the {} MiB limit",
                    crate::protocol::MAX_CLIPBOARD_BYTES / (1024 * 1024)
                ),
                code: None,
            },
        )?;
        return Ok(());
    }
    actions.clipboard_set(&text)
}

/// Handle a `ClipboardGet` message from the guest.
///
/// The bytes pass through exactly as the backend returned them: no
/// trimming, so trailing newlines and indentation survive the round trip.
pub(super) fn handle_clipboard_get(
    stream: &mut UnixStream,
    actions: &dyn HostActions,
) -> anyhow::Result<()> {
    let text = actions.clipboard_get()?;
    if text.len() > crate::protocol::MAX_CLIPBOARD_BYTES {
        write_frame(
            stream,
            &HostMessage::Error {
                reason: format!(
                    "clipboard content exceeds the {} MiB limit",
                    crate::protocol::MAX_CLIPBOARD_BYTES / (1024 * 1024)
                ),
                code: None,
            },
        )?;
        return Ok(());
    }
    let response = HostMessage::ClipboardData { text };
    write_frame(stream, &response)?;
    Ok(())
}

/// Resolve a host-exec request into a canonical executable path, shared
/// by the plain and fd-passing paths. `Err` is the guest-facing message,
/// with the same wording on both paths.
fn resolve_host_exec_request(
    config: &IntegrationConfig,
    cmd: &str,
    args: &[String],
) -> Result<std::path::PathBuf, String> {
    if !config.host_exec.enabled {
        return Err("host-exec is disabled".to_string());
    }

    let entry = match config.host_exec.resolve(cmd) {
        Some(e) => e,
        None => {
            let allowed = config
                .host_exec
                .allowlist
                .as_ref()
                .map(|m| m.keys().cloned().collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            return Err(format!(
                "Permission denied: '{cmd}' is not in the host-exec allowlist\nAllowed commands: {allowed}"
            ));
        }
    };

    // Conditional argument filtering: when `filter = false` the blocklist is
    // bypassed for this command (execution still uses execve, not a shell).
    if entry.filter_enabled() {
        if let Err(msg) = validate_host_exec_args(args) {
            return Err(format!("Security violation: {msg}"));
        }
    } else {
        tracing::debug!(
            "host-exec: security argument filter bypassed for '{cmd}' (filter = false)"
        );
    }
    let resolved = entry.path();

    // Canonicalize the resolved path to mitigate TOCTOU symlink swaps.
    // If the path resolves outside expected system directories (e.g.
    // /nix/store, /usr/bin, etc.), we still allow it — the important thing
    // is that it points to a real regular file right now.
    let canonical_path = match std::fs::canonicalize(resolved) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("host-exec: failed to canonicalize '{}': {e}", resolved);
            return Err(format!(
                "Failed to resolve executable path '{resolved}': {e}"
            ));
        }
    };
    if !canonical_path.is_file() {
        return Err(format!(
            "'{}' is not a regular file",
            canonical_path.display()
        ));
    }
    tracing::info!(
        "host-exec: resolved '{}' -> {}",
        resolved,
        canonical_path.display()
    );
    Ok(canonical_path)
}

/// Handle a `HostExec` message from the guest.
///
/// When the guest negotiated `host_exec_stream`, the child's pipes are
/// forwarded in raw-byte chunks as the command runs, so output larger
/// than a frame (or not valid UTF-8) survives. Otherwise the whole output
/// is buffered and sent as one frame per pipe, exactly as before, for
/// guests that predate streaming.
pub(super) fn handle_host_exec(
    stream: &mut UnixStream,
    config: &IntegrationConfig,
    cmd: String,
    args: Vec<String>,
    streaming: bool,
) -> anyhow::Result<()> {
    let canonical_path = match resolve_host_exec_request(config, &cmd, &args) {
        Ok(p) => p,
        Err(msg) => return host_exec_failed(stream, &msg),
    };

    match Command::new(&canonical_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => {
            if streaming {
                stream_child_output(stream, child, HOST_EXEC_TIMEOUT, MAX_HOST_EXEC_OUTPUT)
            } else {
                buffered_child_output(stream, child)
            }
        }
        Err(e) => {
            let msg = if e.kind() == std::io::ErrorKind::NotFound {
                format!("host-exec: '{cmd}' not found in allowlist path or host $PATH")
            } else {
                format!("host-exec: failed to execute '{cmd}': {e}")
            };
            host_exec_failed(stream, &msg)
        }
    }
}

/// Reply to a rejected host-exec request: the reason on stderr, then done
/// with a failure status. Every rejection funnels through here so the
/// guest always sees a complete stderr/done pair.
fn host_exec_failed(stream: &mut UnixStream, msg: &str) -> anyhow::Result<()> {
    write_frame(
        stream,
        &HostMessage::HostExecStderr {
            data: msg.to_string(),
        },
    )?;
    write_frame(
        stream,
        &HostMessage::HostExecDone {
            exit_code: 1,
            signal: None,
        },
    )?;
    Ok(())
}

/// Exactly how many descriptors a `HostExecV2` request carries: the child
/// reads fd 0 and writes fds 1 and 2. A peer that sends a different count
/// is refused rather than guessed at.
const EXEC_FD_COUNT: usize = 3;

/// How long the host-exec relay loop waits for a peer frame before it
/// reaps the child instead. Short enough for interactive Ctrl-C forwarding
/// when the host is idle.
const SIGNAL_RELAY_POLL: Duration = Duration::from_millis(100);

/// Env keys a guest may export into a host-exec child. The child runs
/// with the host's environment; a guessable key like `LD_PRELOAD` or
/// `PATH` would let container content steer the child differently from
/// the same command run on the host, so only terminal-presentation keys
/// pass through.
const EXEC_ENV_KEYS: &[&str] = &["TERM", "LANG", "COLORTERM"];

/// Filter guest-requested child env down to the fixed allowlist.
fn allowlisted_exec_env(env: &[(String, String)]) -> Vec<(String, String)> {
    env.iter()
        .filter(|(k, _)| EXEC_ENV_KEYS.contains(&k.as_str()))
        .cloned()
        .collect()
}

/// The parsed fields of a `HostExecV2` request, without its descriptors.
pub(super) struct HostExecV2Request {
    pub(super) cmd: String,
    pub(super) args: Vec<String>,
    pub(super) env: Vec<(String, String)>,
    pub(super) cwd: Option<String>,
    pub(super) tty: bool,
}

impl HostExecV2Request {
    pub(super) fn from_message(
        cmd: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: Option<String>,
        tty: bool,
    ) -> Self {
        Self {
            cmd,
            args,
            env,
            cwd,
            tty,
        }
    }
}

/// What a guest cwd request resolves to on the host, after the config's
/// `integration.host_exec.cwd` policy and the container's bind mappings.
enum GuestCwd {
    /// Start the child in the mapped host directory.
    Mapped(String),
    /// Start in the host home: either the policy ignores the guest path
    /// (the default) or a `map` guest path had no mapping and the policy
    /// falls back. `warn` is the stderr note the guest sees through its
    /// own descriptor in the fallback case.
    HostHome { warn: Option<String> },
    /// Run nothing: the policy refuses an unmappable guest path.
    Refused,
}

/// Resolve the configured cwd behaviour for a V2 request.
fn resolve_guest_cwd(cwd: &Option<String>, policy: HostExecCwd, cwd_map: &CwdMap) -> GuestCwd {
    let Some(guest_path) = cwd else {
        // No path carried: nothing to map or refuse, historical default.
        return GuestCwd::HostHome { warn: None };
    };
    match policy {
        HostExecCwd::Home => GuestCwd::HostHome { warn: None },
        HostExecCwd::Map => match cwd_map.translate(guest_path) {
            Some(host) => GuestCwd::Mapped(host),
            None => GuestCwd::HostHome {
                warn: Some(format!(
                    "host-exec: guest working directory '{guest_path}' has no mapping to the \
                     host; starting in the host home\n"
                )),
            },
        },
        HostExecCwd::Refuse => match cwd_map.translate(guest_path) {
            Some(host) => GuestCwd::Mapped(host),
            None => GuestCwd::Refused,
        },
    }
}

/// Handle a `HostExecV2` message: run the allowlisted command with the
/// guest's descriptors as the child's stdio, relay signals to it while it
/// runs, and report its exit.
pub(super) fn handle_host_exec_v2(
    stream: &mut UnixStream,
    config: &Config,
    cwd_map: &CwdMap,
    request: HostExecV2Request,
    fds: Vec<OwnedFd>,
) -> anyhow::Result<()> {
    let HostExecV2Request {
        cmd,
        args,
        env,
        cwd,
        tty,
    } = request;
    // Descriptors are consumed by every path below: moved into the child's
    // stdio, or dropped here — dropping closes them, so a refused request
    // never leaves a stray fd describing guest state in the host process.
    // A tty request carries the slave end of the guest's own pty; the
    // descriptors still flow the same way.
    // The cwd policy is applied before anything runs: a refusal must not
    // execute a command whose working directory the admin did not allow.
    let guest_cwd = resolve_guest_cwd(&cwd, config.integration.host_exec.cwd, cwd_map);
    if let GuestCwd::Refused = &guest_cwd {
        let reason = match &cwd {
            Some(path) => format!(
                "host-exec: refusing to execute with guest working directory '{path}': \
                 no known mapping to the host (integration.host_exec.cwd = \"refuse\")"
            ),
            None => "host-exec: refusing the request".to_string(),
        };
        return host_exec_v2_refused(stream, &reason, "not_allowed");
    }
    let canonical_path = match resolve_host_exec_request(&config.integration, &cmd, &args) {
        Ok(p) => p,
        Err(msg) => return host_exec_failed(stream, &msg),
    };
    let ok_fds = match <Vec<OwnedFd> as TryInto<[OwnedFd; EXEC_FD_COUNT]>>::try_into(fds) {
        Ok(f) => f,
        Err(_) => {
            return host_exec_v2_refused(
                stream,
                &format!(
                    "host-exec: the request must carry exactly {EXEC_FD_COUNT} descriptors (stdin, stdout, stderr)"
                ),
                "bad_request",
            );
        }
    };

    // Security model: these descriptors arrived from the guest over the
    // host connection and are guest-side pipes, files and terminals —
    // plumbing between the guest's own processes. With them as its stdio
    // the host child reads and writes what the guest asked it to, and the
    // host gains no access it did not already have for running the
    // allowlisted command. The delivery copies are consumed by the child's
    // stdio and dropped right after spawn, so fds are closed here rather
    // than leaked into later requests.
    let [child_stdin, child_stdout, child_stderr] = ok_fds;

    // Only the allowlisted keys reach through; the child otherwise runs in
    // the host daemon's own environment. The cwd policy decides where the
    // child starts: `home` (the default) starts in the host home; `map`
    // starts it in the host directory the guest cwd is bound from, with a
    // stderr warning (through the guest's own descriptor, so it reaches
    // attached-stdio pipes and ptys alike) when the guest path has no
    // mapping; `refuse` never got this far.
    let mut command = Command::new(&canonical_path);
    command.args(&args);
    for (key, value) in allowlisted_exec_env(&env) {
        command.env(key, value);
    }
    match guest_cwd {
        GuestCwd::Mapped(host_dir) => {
            tracing::info!("host-exec: starting child in mapped host cwd {host_dir:?}");
            command.current_dir(host_dir);
        }
        GuestCwd::HostHome { warn } => {
            // The fallback warning goes through the guest's own stderr
            // descriptor, before any child output: no protocol frame is
            // needed and both the pipe and pty session shapes deliver it.
            if let Some(text) = warn {
                // Writing through nix keeps the fd semantics explicit and
                // avoids depending on `OwnedFd`'s io traits age.
                nix::unistd::write(&child_stderr, text.as_bytes()).ok();
            }
            command.current_dir(cwd_map.host_home());
        }
        // Unreachable: a refusal returned above, and there is no path
        // that resolves to `Refused` twice.
        GuestCwd::Refused => {}
    }
    // Own process group for pipe sessions: a relayed SIGINT/SIGTERM reaches
    // the child and nothing above it, unlike a child sharing the daemon's
    // group. A tty child gets its own session instead (ties the terminal
    // to it and gives it a group of its pid anyway), and must NOT be made
    // a group leader here, or the session creation fails.
    if tty {
        give_the_child_its_tty(&mut command, &child_stdin);
    } else {
        command.process_group(0);
    }
    let child = command
        .stdin(Stdio::from(child_stdin))
        .stdout(Stdio::from(child_stdout))
        .stderr(Stdio::from(child_stderr))
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            let msg = if e.kind() == std::io::ErrorKind::NotFound {
                format!("host-exec: '{cmd}' not found in allowlist path or host $PATH")
            } else {
                format!("host-exec: failed to execute '{cmd}': {e}")
            };
            return host_exec_failed(stream, &msg);
        }
    };

    // Spawn succeeded: the stdio descriptors are in the child now, and the
    // host's own copies are closed by `Stdio`'s drop after `spawn` — no
    // leaked fd holds the guest's pipes open past the child's lifetime.
    // The per-entry timeout kills only this child's group, and is reported
    // as a signal death like any other kill; a session with no configured
    // timeout is never time-limited here, and neither is a tty session —
    // an interactive child outliving its request is correct behaviour.
    let timeout = if tty {
        None
    } else {
        config
            .integration
            .host_exec
            .resolve(&cmd)
            .and_then(|entry| entry.timeout_secs())
            .map(Duration::from_secs)
    };
    run_child_with_signal_relay(stream, &mut child, timeout)
}

/// Register the tty child's session setup on the command: `setsid` makes
/// the child a session leader — its own process group, so the relay's
/// `killpg` target is exactly this child — and `TIOCSCTTY` on the stdin
/// it inherits makes the guest's pty its controlling terminal, so the
/// kernel exercises the terminal's own Ctrl-C, job control and
/// window-size accounting directly on the child.
///
/// Must not be combined with `process_group(0)`: a group leader cannot
/// `setsid`, and the session creation would fail before the exec.
fn give_the_child_its_tty(command: &mut Command, ctty: &OwnedFd) {
    let ctty_fd = ctty.as_raw_fd();
    // SAFETY: `pre_exec` runs the closure between fork and exec, where
    // only async-signal-safe code may touch the world. `setsid` and
    // `ioctl` are raw syscalls (nix and libc wrap them in direct calls,
    // no allocation, no locks), the descriptor used is the slave the
    // child inherits as stdin, and a failure aborts the spawn with an
    // error instead of leaving a half-configured child behind.
    //
    // The workspace denies `unsafe` wholesale; this session-leader
    // kernel setup has no safe wrapper, as documented above.
    #[allow(unsafe_code)]
    // SAFETY: see above.
    unsafe {
        command.pre_exec(move || {
            nix::unistd::setsid().map_err(std::io::Error::from)?;
            if nix::libc::ioctl(ctty_fd, nix::libc::TIOCSCTTY, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Refuse a `HostExecV2` request before anything runs: a typed `Error`
/// frame, with the request's descriptors (if any) dropped by the caller of
/// this handler not being run.
fn host_exec_v2_refused(stream: &mut UnixStream, reason: &str, code: &str) -> anyhow::Result<()> {
    write_frame(
        stream,
        &HostMessage::Error {
            reason: reason.to_string(),
            code: Some(code.to_string()),
        },
    )?;
    Ok(())
}

/// While a V2 child runs, the connection carries relay messages only:
/// poll the peer for `HostExecSignal` frames with a short timeout and
/// reap the child in the gaps. On any disconnect or foreign frame with
/// attached descriptors, stray descriptors are closed, and the child is
/// killed and reaped so nothing outlives its session.
///
/// `timeout` is the allowlist entry's configured execution timeout: once
/// it expires the child's process group is SIGKILLed and the death is
/// reported like any signal death (`128 + 9`, signal 9). `None` — the
/// default for entries without `timeout` — never time-limits the child.
fn run_child_with_signal_relay(
    stream: &mut UnixStream,
    child: &mut std::process::Child,
    timeout: Option<Duration>,
) -> anyhow::Result<()> {
    let pgid = Pid::from_raw(child.id().cast_signed());
    let mut reader = FrameReader::new();
    stream.set_read_timeout(Some(SIGNAL_RELAY_POLL))?;
    let deadline = timeout.map(|t| std::time::Instant::now() + t);

    let mut exited: Option<std::process::ExitStatus> = None;
    while exited.is_none() {
        // A peer that attaches descriptors to a relay frame misuses the
        // session: capture and close them instead of leaving them for a
        // later (different) request.
        let mut stray: Vec<OwnedFd> = Vec::new();
        let read = reader.read(&mut FdCapturingReader::new(stream, &mut stray));
        drop(stray);
        match read {
            Ok(FrameRead::Frame(bytes)) => {
                if let Ok(GuestMessage::HostExecSignal { sig }) =
                    serde_json::from_slice::<GuestMessage>(&bytes)
                {
                    relay_signal(pgid, sig);
                } else {
                    tracing::debug!("host-exec: ignoring non-relay frame while child runs");
                }
            }
            Ok(FrameRead::Eof) => {
                // The shim died without reporting: nothing is left to
                // write output to, so the child should not keep running.
                tracing::warn!("host-exec: peer closed the connection; killing the child");
                let _ = kill(pgid, Signal::SIGKILL);
                child.wait()?;
                stream.set_read_timeout(Some(PING_INTERVAL))?;
                return Ok(());
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                match child.try_wait() {
                    Ok(Some(status)) => exited = Some(status),
                    Ok(None) => {
                        // A child that exits inside the poll gap is
                        // reaped normally: the deadline never kills a
                        // command that finished on its own. Only a live
                        // child past its deadline gets the kill.
                        if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                            tracing::warn!(
                                "host-exec: allowlist entry timeout expired; killing the child"
                            );
                            let _ = killpg(pgid, Signal::SIGKILL);
                        }
                    }
                    // A reap failure means the child is gone and unknown:
                    // fail loudly, the caller treats it as host drift.
                    Err(e) => {
                        stream.set_read_timeout(Some(PING_INTERVAL))?;
                        return Err(anyhow::Error::from(e));
                    }
                }
            }
            Err(e) => {
                tracing::warn!("host-exec: relay read failed: {e}");
                let _ = kill(pgid, Signal::SIGKILL);
                let _ = child.wait();
                stream.set_read_timeout(Some(PING_INTERVAL))?;
                return Err(anyhow::Error::from(e));
            }
        }
    }

    stream.set_read_timeout(Some(PING_INTERVAL))?;
    let (exit_code, signal) = exit_code_and_signal(exited.as_ref().unwrap());
    write_frame(stream, &HostMessage::HostExecDone { exit_code, signal })?;
    Ok(())
}

/// Map a guest-relayed signal number to the child's process group. Only
/// the shim's known set is relayed (SIGHUP, SIGINT, SIGTERM, SIGWINCH):
/// `HostExecSignal` is a relay channel for terminal signals, not an
/// arbitrary host API, so any other number is dropped with a log. Relay
/// errors are debug-logged, since a child that has just exited races any
/// late `HostExecSignal` with an unavoidable `ESRCH`.
fn relay_signal(pgid: Pid, sig: i32) {
    let signal = match sig {
        1 => Signal::SIGHUP,
        2 => Signal::SIGINT,
        15 => Signal::SIGTERM,
        28 => Signal::SIGWINCH,
        other => {
            tracing::debug!("host-exec: ignoring unforwardable signal {other}");
            return;
        }
    };
    // The child leads its own process group (`process_group(0)` at
    // spawn), so the relay reaches only it — not the daemon's group —
    // however the guest shim was itself signalled.
    match killpg(pgid, signal) {
        Err(nix::errno::Errno::ESRCH) => {}
        Err(e) => {
            tracing::debug!("host-exec: relay of signal {} failed: {e}", signal as i32)
        }
        Ok(()) => {
            tracing::debug!(
                "host-exec: relayed signal {} to the child's group",
                signal as i32
            )
        }
    }
}

/// Wait for the child, then send its whole stdout and stderr as one frame
/// each. Preserves the historical framing for guests that did not
/// negotiate `host_exec_stream`.
fn buffered_child_output(
    stream: &mut UnixStream,
    child: std::process::Child,
) -> anyhow::Result<()> {
    let output = crate::process::wait_child_timeout(child, HOST_EXEC_TIMEOUT)?;
    if !output.stdout.is_empty() {
        write_frame(
            stream,
            &HostMessage::HostExecStdout {
                data: String::from_utf8_lossy(&output.stdout).to_string(),
            },
        )?;
    }
    if !output.stderr.is_empty() {
        write_frame(
            stream,
            &HostMessage::HostExecStderr {
                data: String::from_utf8_lossy(&output.stderr).to_string(),
            },
        )?;
    }
    let (exit_code, signal) = exit_code_and_signal(&output.status);
    write_frame(stream, &HostMessage::HostExecDone { exit_code, signal })?;
    Ok(())
}

/// Exit status split into a shell-convention code and the killing signal,
/// if any: a normal exit reports its code with no signal, a signal death
/// reports `128 + signal` alongside the signal number.
pub(super) fn exit_code_and_signal(status: &std::process::ExitStatus) -> (i32, Option<i32>) {
    use std::os::unix::process::ExitStatusExt as _;
    match (status.code(), status.signal()) {
        (Some(code), _) => (code, None),
        (None, signal) => (signal.map(|s| 128 + s).unwrap_or(1), signal),
    }
}

/// One item moving from a streaming child's pipe readers to the sender.
enum ExecChunk {
    Out(Vec<u8>),
    Err(Vec<u8>),
    OutEof,
    ErrEof,
}

/// Forward a streaming child's pipes as raw-byte chunks until both hit
/// EOF, then report the exit. Kills the child when `timeout` elapses or
/// `max_bytes` of total output is exceeded.
pub(super) fn stream_child_output(
    stream: &mut UnixStream,
    mut child: std::process::Child,
    timeout: Duration,
    max_bytes: u64,
) -> anyhow::Result<()> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel::<ExecChunk>();
    if let Some(pipe) = stdout {
        pump_pipe(pipe, tx.clone(), ExecChunk::Out, ExecChunk::OutEof);
    }
    if let Some(pipe) = stderr {
        pump_pipe(pipe, tx.clone(), ExecChunk::Err, ExecChunk::ErrEof);
    }
    drop(tx);

    // Same killer as the buffered path: SIGKILL past the timeout. The done
    // signal disarms it once the child has been reaped.
    let pid = Pid::from_raw(child.id().cast_signed());
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        if done_rx.recv_timeout(timeout).is_err() {
            let _ = kill(pid, Signal::SIGKILL);
        }
    });

    let mut eofs = 0;
    let mut total: u64 = 0;
    while eofs < 2 {
        let (payload, wire_stream) = match rx.recv() {
            Ok(ExecChunk::Out(data)) => (data, ExecStream::Stdout),
            Ok(ExecChunk::Err(data)) => (data, ExecStream::Stderr),
            Ok(ExecChunk::OutEof | ExecChunk::ErrEof) => {
                eofs += 1;
                continue;
            }
            // Readers only exit after sending their EOF; a dead channel
            // here means both are gone.
            Err(_) => break,
        };
        total = total.saturating_add(payload.len() as u64);
        if total > max_bytes {
            tracing::warn!("host-exec: output exceeded {max_bytes} bytes, killing child");
            let _ = kill(Pid::from_raw(child.id().cast_signed()), Signal::SIGKILL);
            write_frame(
                stream,
                &HostMessage::HostExecStderr {
                    data: format!("host-exec: output exceeded {max_bytes} bytes, truncated"),
                },
            )?;
            // Reap the killed child; unread pipe data dies with the
            // readers, which exit on EOF by themselves.
            let _ = child.wait()?;
            write_frame(
                stream,
                &HostMessage::HostExecDone {
                    exit_code: 1,
                    signal: None,
                },
            )?;
            let _ = done_tx.send(());
            return Ok(());
        }
        write_frame(
            stream,
            &HostMessage::HostExecOutput {
                stream: wire_stream,
                data: payload,
            },
        )?;
    }
    let status = child.wait()?;
    let _ = done_tx.send(());
    let (exit_code, signal) = exit_code_and_signal(&status);
    write_frame(stream, &HostMessage::HostExecDone { exit_code, signal })?;
    Ok(())
}

/// Copy one child pipe into the channel in 16 KiB chunks until EOF, then
/// report EOF. A broken pipe (child killed) ends the stream like an EOF.
fn pump_pipe<R: std::io::Read + Send + 'static>(
    mut pipe: R,
    tx: std::sync::mpsc::Sender<ExecChunk>,
    wrap: fn(Vec<u8>) -> ExecChunk,
    eof: ExecChunk,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; EXEC_CHUNK_LEN];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) => {
                    let _ = tx.send(eof);
                    return;
                }
                Ok(n) => {
                    if tx.send(wrap(buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
                Err(_) => {
                    let _ = tx.send(eof);
                    return;
                }
            }
        }
    });
}

/// Validate arguments for host-exec, rejecting shell metacharacters and
/// dangerous flag patterns that could alter the behaviour of a whitelisted
/// binary (e.g. `git --exec-path=…`).
///
/// # Security model and limitations
///
/// Arguments are validated with a substring blocklist. Commands run via
/// `execve` directly — **not** through `/bin/sh` — so metacharacters like
/// `;`, `|`, `$(` cannot cause shell injection on ordinary ELF binaries.
/// The blocklist exists to reduce misuse, not to make arbitrary allowlist
/// entries safe.
///
/// Known bypass classes that this filter **cannot** prevent:
///
/// * Versatile binaries: any program with code-execution or file-access
///   flags defeats substring filtering regardless of metacharacters —
///   e.g. `git -C /root …`, `git clone --upload-pack=…`, `find -exec …`,
///   `tar --to-command=…`, `python -c …`, `ssh -oProxyCommand=…`.
/// * Flag synonyms: only a small set of dangerous prefixes is known; an
///   allowlisted binary may expose others (`--pager`, `-c`, `--eval`, …).
///
/// Therefore:
///
/// * Allowlist only restricted binaries or dedicated wrapper scripts.
/// * Prefer wrappers that pin the arguments (e.g. a script exposing exactly
///   `systemctl --user status <unit>`), rather than raw `git`, `python`,
///   `tar`, `find`, or shells.
/// * Treat every allowlist entry as granting the guest that binary's full
///   capability surface on the host.
///
/// False positives are expected: benign messages containing `<()`, globs,
/// or parentheses are rejected. Affected users should route those commands
/// through a wrapper script instead of loosening this filter.
pub(super) fn validate_host_exec_args(args: &[String]) -> Result<(), String> {
    for arg in args {
        if arg.contains(';')
            || arg.contains('|')
            || arg.contains('&')
            || arg.contains('$')
            || arg.contains('`')
            || arg.contains('\n')
            || arg.contains('\r')
        {
            return Err(format!("argument {arg:?} contains shell metacharacters"));
        }
        if arg.contains('<') || arg.contains('>') {
            return Err(format!("argument {arg:?} contains redirection operators"));
        }
        if arg.contains('*')
            || arg.contains('?')
            || arg.contains('[')
            || arg.contains(']')
            || arg.contains('{')
            || arg.contains('}')
        {
            return Err(format!(
                "argument {arg:?} contains glob or brace characters"
            ));
        }
        if arg.contains('(') || arg.contains(')') || arg.contains('\\') {
            return Err(format!(
                "argument {arg:?} contains subshell or escape characters"
            ));
        }
        let lower = arg.to_ascii_lowercase();
        if lower.starts_with("--exec-path")
            || lower.starts_with("--config")
            || lower.starts_with("--plugin")
            || lower.starts_with("--load")
            || lower.starts_with("--module")
            || lower.starts_with("--remote=")
            || lower == "-o"
        {
            return Err(format!("argument {arg:?} uses a restricted flag pattern"));
        }
    }
    Ok(())
}
