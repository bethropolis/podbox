//! Per-message protocol handlers for the host socket server (`hello`,
//! `host-exec`, `notify`, `xdg-open`, `clipboard`). Single cohesive concern;
//! stays above ~300 LOC because the message-dispatch contract is one unit
//! (documented exemption, per the modularization guide 1/8).
use std::os::unix::net::UnixStream;
use std::time::Duration;

use super::actions::HostActions;
use crate::config::IntegrationConfig;
use crate::protocol::{HostMessage, write_frame};

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
            crate::protocol::CAP_XDG_OPEN => config.xdg_open,
            crate::protocol::CAP_CLIPBOARD => config.clipboard,
            crate::protocol::CAP_HOST_EXEC => config.host_exec.enabled,
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
    choices: Vec<crate::protocol::NotifyAction>,
) -> anyhow::Result<()> {
    if choices.is_empty() {
        if let Err(e) = actions.notify(&summary, &body, &choices) {
            tracing::debug!("notification not shown: {e:#}");
        }
        return Ok(());
    }
    let action_key = actions
        .notify(&summary, &body, &choices)
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

/// Schemes the host will open with `xdg-open`. Anything else is refused.
///
/// An allowlist, not a blocklist: a blocklist misses every handler the host
/// has installed (`smb:`, `vscode:`, `steam:`, `trash:`, ...), and each one is
/// a way for guest input to reach a host app or a network share.
pub(crate) const ALLOWED_URI_SCHEMES: &[&str] = &["http", "https", "mailto"];

/// Longest URI accepted from the guest.
pub(crate) const MAX_URI_LEN: usize = 8192;

/// Why a guest-supplied URI was refused.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UriError {
    Empty,
    TooLong,
    ControlCharacter,
    Malformed,
    SchemeNotAllowed(String),
    MissingHost,
}

impl std::fmt::Display for UriError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty URI"),
            Self::TooLong => write!(f, "URI longer than {MAX_URI_LEN} bytes"),
            Self::ControlCharacter => write!(f, "URI contains a control character"),
            Self::Malformed => write!(f, "not a URI"),
            Self::SchemeNotAllowed(scheme) => write!(
                f,
                "scheme '{scheme}' is not allowed (allowed: {})",
                ALLOWED_URI_SCHEMES.join(", ")
            ),
            Self::MissingHost => write!(f, "web URL has no host"),
        }
    }
}

/// Check a guest-supplied URI and return the text to open.
///
/// A bare host such as `example.com` is opened over https. Paths and anything
/// that is not a URI are refused.
pub(crate) fn validate_uri(uri: &str) -> Result<String, UriError> {
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
    if !ALLOWED_URI_SCHEMES.contains(&scheme.as_str()) {
        return Err(UriError::SchemeNotAllowed(scheme));
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
    uri: String,
) -> anyhow::Result<()> {
    match validate_uri(&uri) {
        Ok(validated) => actions.open_uri(&validated)?,
        Err(e) => {
            tracing::warn!("refused xdg-open request: {e}");
            write_frame(
                stream,
                &HostMessage::Error {
                    reason: format!("xdg-open refused: {e}"),
                },
            )?;
        }
    }
    Ok(())
}

/// Handle a `ClipboardSet` message from the guest.
pub(super) fn handle_clipboard_set(actions: &dyn HostActions, text: String) -> anyhow::Result<()> {
    actions.clipboard_set(&text)
}

/// Handle a `ClipboardGet` message from the guest.
pub(super) fn handle_clipboard_get(
    stream: &mut UnixStream,
    actions: &dyn HostActions,
) -> anyhow::Result<()> {
    let text = actions.clipboard_get()?;
    let response = HostMessage::ClipboardData {
        text: text.trim().to_string(),
    };
    write_frame(stream, &response)?;
    Ok(())
}

/// Handle a `HostExec` message from the guest.
pub(super) fn handle_host_exec(
    stream: &mut UnixStream,
    config: &IntegrationConfig,
    cmd: String,
    args: Vec<String>,
) -> anyhow::Result<()> {
    if !config.host_exec.enabled {
        write_frame(
            stream,
            &HostMessage::HostExecStderr {
                data: "host-exec is disabled".into(),
            },
        )?;
        write_frame(stream, &HostMessage::HostExecDone { exit_code: 1 })?;
        return Ok(());
    }

    let entry = match config.host_exec.resolve(&cmd) {
        Some(e) => e,
        None => {
            let allowed = config
                .host_exec
                .allowlist
                .as_ref()
                .map(|m| m.keys().cloned().collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            write_frame(
                stream,
                &HostMessage::HostExecStderr {
                    data: format!(
                        "Permission denied: '{cmd}' is not in the host-exec allowlist\nAllowed commands: {allowed}"
                    ),
                },
            )?;
            write_frame(stream, &HostMessage::HostExecDone { exit_code: 1 })?;
            return Ok(());
        }
    };

    // Conditional argument filtering: when `filter = false` the blocklist is
    // bypassed for this command (execution still uses execve, not a shell).
    if entry.filter_enabled() {
        if let Err(msg) = validate_host_exec_args(&args) {
            write_frame(
                stream,
                &HostMessage::HostExecStderr {
                    data: format!("Security violation: {msg}"),
                },
            )?;
            write_frame(stream, &HostMessage::HostExecDone { exit_code: 1 })?;
            return Ok(());
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
            write_frame(
                stream,
                &HostMessage::HostExecStderr {
                    data: format!("Failed to resolve executable path '{resolved}': {e}"),
                },
            )?;
            write_frame(stream, &HostMessage::HostExecDone { exit_code: 1 })?;
            return Ok(());
        }
    };
    if !canonical_path.is_file() {
        write_frame(
            stream,
            &HostMessage::HostExecStderr {
                data: format!("'{}' is not a regular file", canonical_path.display()),
            },
        )?;
        write_frame(stream, &HostMessage::HostExecDone { exit_code: 1 })?;
        return Ok(());
    }
    tracing::info!(
        "host-exec: resolved '{}' -> {}",
        resolved,
        canonical_path.display()
    );

    match std::process::Command::new(&canonical_path)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => {
            let output = crate::process::wait_child_timeout(child, Duration::from_mins(1))?;
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
            let code = output.status.code().unwrap_or(1);
            write_frame(stream, &HostMessage::HostExecDone { exit_code: code })?;
        }
        Err(e) => {
            let msg = if e.kind() == std::io::ErrorKind::NotFound {
                format!("host-exec: '{cmd}' not found in allowlist path or host $PATH")
            } else {
                format!("host-exec: failed to execute '{cmd}': {e}")
            };
            write_frame(stream, &HostMessage::HostExecStderr { data: msg })?;
            write_frame(stream, &HostMessage::HostExecDone { exit_code: 1 })?;
        }
    }
    Ok(())
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
