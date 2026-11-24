use std::collections::HashSet;
use std::os::unix::net::UnixStream;

use crate::config::IntegrationConfig;
use crate::protocol::{HostMessage, write_frame};

use super::MAX_NEGOTIATION_FAILURES;

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
        host_version: crate::VERSION.into(),
    };
    write_frame(stream, &response)?;
    Ok(HelloOutcome::Accepted(accepted))
}

/// Count a failed negotiation and reply with a typed `Error` frame. Returns
/// true once the connection should be dropped (fail-closed after
/// `MAX_NEGOTIATION_FAILURES` failures).
pub(crate) fn note_failure(stream: &mut UnixStream, failures: &mut u32, reason: &str) -> bool {
    *failures = failures.saturating_add(1);
    let _ = write_frame(
        stream,
        &HostMessage::Error {
            reason: reason.to_string(),
            code: None,
        },
    );
    *failures >= MAX_NEGOTIATION_FAILURES
}

/// True if `negotiated` contains the given capability.
pub(crate) fn has_cap(negotiated: &Option<HashSet<String>>, cap: &str) -> bool {
    negotiated
        .as_ref()
        .is_some_and(|caps| caps.iter().any(|c| c == cap))
}
