use std::env;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use crate::error::GuestError;
use crate::protocol::{GuestMessage, HostMessage, read_frame, write_frame};

/// Container name from the environment.
pub fn container_name() -> Result<String, GuestError> {
    env::var("PODBOX_CONTAINER")
        .or_else(|_| env::var("PODMGR_CONTAINER"))
        .map_err(|_| GuestError::ContainerNameMissing)
}

/// Host socket path inside the container.
pub fn host_socket_path() -> Result<PathBuf, GuestError> {
    let cn = container_name()?;
    let socket_dir = podbox_protocol::sysutil::xdg_runtime_dir().join("podbox");
    Ok(socket_dir.join(format!("{cn}.sock")))
}

/// Connect to the host socket with retries using exponential backoff.
pub fn connect_to_host(socket_path: &Path) -> Result<UnixStream, GuestError> {
    let mut delay = std::time::Duration::from_millis(100);
    for attempt in 1..=4 {
        match UnixStream::connect(socket_path) {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                eprintln!(
                    "Socket connect attempt {}/4 failed: {} ({})",
                    attempt,
                    socket_path.display(),
                    e
                );
                if attempt < 4 {
                    std::thread::sleep(delay);
                    delay *= 3;
                }
            }
        }
    }
    Err(GuestError::Io(std::io::Error::new(
        std::io::ErrorKind::NotConnected,
        "failed to connect to host socket after 4 attempts",
    )))
}

/// Perform the hello handshake.
///
/// Returns (`accepted_capabilities`, `idle_timeout_secs`, `host_exec_shims`).
pub fn handshake(
    host_stream: &mut UnixStream,
    container_name: &str,
    capabilities: &[String],
) -> Result<(Vec<String>, u64, Vec<String>), GuestError> {
    let hello = GuestMessage::Hello {
        protocol_version: crate::protocol::PROTOCOL_VERSION,
        guest_version: crate::VERSION.into(),
        container: container_name.into(),
        capabilities: capabilities.to_vec(),
    };
    write_frame(host_stream, &hello)?;

    let response = read_frame(host_stream)?;
    let response = response.ok_or(GuestError::HandshakeFailed(
        "host closed connection during handshake".into(),
    ))?;

    let msg: HostMessage = serde_json::from_slice(&response)?;

    match msg {
        HostMessage::HelloAck {
            accepted,
            idle_timeout_secs,
            host_exec_shims,
            ..
        } => Ok((accepted, idle_timeout_secs, host_exec_shims)),
        // The host answers with `Shutdown` when the protocol versions do
        // not match (see the host's hello handler). Retrying that
        // handshake is pointless, so it gets its own error: the caller
        // must not reconnect.
        HostMessage::Shutdown => Err(GuestError::VersionMismatch(
            "host uses an incompatible protocol version".into(),
        )),
        _ => Err(GuestError::HandshakeFailed(
            "unexpected response from host".into(),
        )),
    }
}

/// Connect to the host socket and negotiate `caps` before anything else.
///
/// The host rejects every privileged message on a connection that has not
/// completed `Hello`, so every guest-side request must go through here.
/// Fails with [`GuestError::CapabilityRefused`] if the host did not grant
/// one of the requested capabilities.
pub fn connect_negotiated(caps: &[&str]) -> Result<UnixStream, GuestError> {
    let socket_path = host_socket_path()?;
    let container = container_name()?;
    negotiate_at(&socket_path, &container, caps)
}

/// [`connect_negotiated`] with the socket path and container name supplied
/// by the caller, so tests need not touch the process environment.
///
/// Makes a single connect attempt and fails immediately. Interceptors are
/// one-shot and short-lived, so they should not sit in a retry loop; the
/// daemon keeps its own retry logic via [`connect_to_host`].
pub fn negotiate_at(
    socket_path: &Path,
    container: &str,
    caps: &[&str],
) -> Result<UnixStream, GuestError> {
    negotiate_at_with_optional(socket_path, container, caps, &[]).map(|(stream, _)| stream)
}

/// [`negotiate_at`] with optional capabilities: `required` must all be
/// granted (else [`GuestError::CapabilityRefused`]), while `optional` ones
/// are taken when granted and ignored when refused. Returns the stream and
/// the full accepted list so the caller can adapt to what the host
/// supports — e.g. streaming host-exec output when the host grants
/// `host_exec_stream`, buffered frames otherwise.
pub fn negotiate_at_with_optional(
    socket_path: &Path,
    container: &str,
    required: &[&str],
    optional: &[&str],
) -> Result<(UnixStream, Vec<String>), GuestError> {
    let mut stream = UnixStream::connect(socket_path)?;
    let mut requested: Vec<String> = required.iter().map(|c| (*c).to_string()).collect();
    requested.extend(optional.iter().map(|c| (*c).to_string()));
    let (accepted, _idle_timeout, _shims) = handshake(&mut stream, container, &requested)?;
    for cap in required {
        if !accepted.iter().any(|a| a == cap) {
            return Err(GuestError::CapabilityRefused((*cap).to_string()));
        }
    }
    Ok((stream, accepted))
}

/// Negotiate `cap`, send one message on the same connection, and close.
pub fn connect_and_send_oneshot(msg: &GuestMessage, cap: &str) -> Result<(), GuestError> {
    let mut stream = connect_negotiated(&[cap])?;
    write_frame(&mut stream, msg)?;
    Ok(())
}

/// Read a host message from the stream.
pub fn read_host_message(stream: &mut UnixStream) -> Result<Option<HostMessage>, GuestError> {
    match read_frame(stream)? {
        Some(bytes) => {
            let msg: HostMessage = serde_json::from_slice(&bytes)?;
            Ok(Some(msg))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Shutdown` reply to `Hello` means the protocol versions do not
    /// match. It must surface as a fatal error (no reconnect), not as a
    /// retryable handshake failure.
    #[test]
    fn shutdown_reply_is_a_version_mismatch_not_a_retryable_error() {
        let (mut guest, mut mock) = UnixStream::pair().unwrap();
        std::thread::spawn(move || {
            let bytes = read_frame(&mut mock).unwrap().expect("hello frame");
            let hello: GuestMessage = serde_json::from_slice(&bytes).unwrap();
            assert!(matches!(hello, GuestMessage::Hello { .. }));
            write_frame(&mut mock, &HostMessage::Shutdown).unwrap();
        });
        let err = handshake(&mut guest, "dev", &["notify".to_string()]).unwrap_err();
        assert!(
            matches!(err, GuestError::VersionMismatch(_)),
            "expected VersionMismatch, got {err:?}"
        );
    }
}
