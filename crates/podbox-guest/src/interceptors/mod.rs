pub mod clipboard;
pub mod host_exec;
pub mod notify;
pub mod xdg_open;

use crate::protocol::{GuestMessage, HostMessage, read_frame, write_frame};
use crate::socket;

/// Negotiate `cap`, send a message, and do not wait for a response.
///
/// The host never acks notifications, xdg-open, or clipboard-set frames.
fn send_to_host(cap: &str, msg: &GuestMessage) -> Result<(), crate::error::GuestError> {
    let mut stream = socket::connect_negotiated(&[cap])?;
    write_frame(&mut stream, msg)?;
    Ok(())
}

/// Negotiate `cap`, send a message, and read the typed host response.
fn send_to_host_and_read_response(
    cap: &str,
    msg: &GuestMessage,
) -> Result<HostMessage, crate::error::GuestError> {
    let mut stream = socket::connect_negotiated(&[cap])?;
    write_frame(&mut stream, msg)?;
    let response = read_frame(&mut stream)?.ok_or_else(|| {
        crate::error::GuestError::HandshakeFailed("host closed connection".into())
    })?;
    let host_msg: HostMessage = serde_json::from_slice(&response)?;
    Ok(host_msg)
}
