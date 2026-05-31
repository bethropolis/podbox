use std::os::fd::{AsRawFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;

use nix::poll::{poll, PollFd, PollFlags, PollTimeout};

use crate::error::GuestError;
use crate::protocol::{read_frame, write_frame, GuestMessage, HostMessage};

/// Connect to the host socket with retries using poll-based backoff.
pub fn connect_to_host(socket_path: &Path) -> Result<UnixStream, GuestError> {
    for attempt in 1..=3 {
        match UnixStream::connect(socket_path) {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                eprintln!(
                    "Socket connect attempt {}/3 failed: {} ({})",
                    attempt,
                    socket_path.display(),
                    e
                );
                if attempt < 3 {
                    let dev_null = std::fs::File::open("/dev/null")
                        .map_err(GuestError::IO)?;
                    let fd = unsafe { BorrowedFd::borrow_raw(dev_null.as_raw_fd()) };
                    let mut poll_fds = [PollFd::new(fd, PollFlags::empty())];
                    let _ = poll(&mut poll_fds, PollTimeout::from(500u16));
                }
            }
        }
    }
    Err(GuestError::SocketError(std::io::Error::new(
        std::io::ErrorKind::NotConnected,
        "failed to connect to host socket after 3 attempts",
    )))
}

/// Perform the hello handshake.
pub fn handshake(
    host_stream: &mut UnixStream,
    container_name: &str,
    capabilities: &[String],
) -> Result<Vec<String>, GuestError> {
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
        HostMessage::HelloAck { accepted, .. } => Ok(accepted),
        _ => Err(GuestError::HandshakeFailed(
            "unexpected response from host".into(),
        )),
    }
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
