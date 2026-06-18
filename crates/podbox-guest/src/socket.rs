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
    let xdg_runtime = env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", nix::unistd::getuid()));
    Ok(PathBuf::from(&xdg_runtime)
        .join("podbox")
        .join(format!("{cn}.sock")))
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
/// Returns (`accepted_capabilities`, `idle_timeout_secs`).
pub fn handshake(
    host_stream: &mut UnixStream,
    container_name: &str,
    capabilities: &[String],
) -> Result<(Vec<String>, u64), GuestError> {
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
            ..
        } => Ok((accepted, idle_timeout_secs)),
        _ => Err(GuestError::HandshakeFailed(
            "unexpected response from host".into(),
        )),
    }
}

/// Open a fresh connection to the host socket, send one message, and close.
pub fn connect_and_send_oneshot(msg: &GuestMessage) -> Result<(), GuestError> {
    let socket_path = host_socket_path()?;
    let mut stream = UnixStream::connect(&socket_path)?;
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
