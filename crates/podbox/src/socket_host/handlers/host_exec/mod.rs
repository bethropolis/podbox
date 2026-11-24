use std::os::unix::net::UnixStream;

use crate::config::IntegrationConfig;
use crate::protocol::{HostMessage, write_frame};

mod legacy;
mod v2;

pub(in crate::socket_host) use legacy::handle_host_exec;
#[cfg(test)]
pub(in crate::socket_host) use legacy::stream_child_output;
pub(in crate::socket_host) use v2::{HostExecV2Request, handle_host_exec_v2};

/// Resolve a host-exec request into a canonical executable path, shared
/// by the plain and fd-passing paths. `Err` is the guest-facing message,
/// with the same wording on both paths.
pub(super) fn resolve_host_exec_request(
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

/// Reply to a rejected host-exec request: the reason on stderr, then done
/// with a failure status. Every rejection funnels through here so the
/// guest always sees a complete stderr/done pair.
pub(super) fn host_exec_failed(stream: &mut UnixStream, msg: &str) -> anyhow::Result<()> {
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

/// Filter guest-requested child env down to the shared presentation
/// allowlist. A key like `LD_PRELOAD` or `PATH` must not let container
/// content steer a host child differently from the same host command.
pub(super) fn allowlisted_exec_env(env: &[(String, String)]) -> Vec<(String, String)> {
    env.iter()
        .filter(|(k, _)| crate::protocol::is_host_exec_env(k))
        .cloned()
        .collect()
}

/// Exit status split into a shell-convention code and the killing signal,
/// if any: a normal exit reports its code with no signal, a signal death
/// reports `128 + signal` alongside the signal number.
pub(in crate::socket_host) fn exit_code_and_signal(
    status: &std::process::ExitStatus,
) -> (i32, Option<i32>) {
    use std::os::unix::process::ExitStatusExt as _;
    match (status.code(), status.signal()) {
        (Some(code), _) => (code, None),
        (None, signal) => (signal.map(|s| 128 + s).unwrap_or(1), signal),
    }
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
pub(in crate::socket_host) fn validate_host_exec_args(args: &[String]) -> Result<(), String> {
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
