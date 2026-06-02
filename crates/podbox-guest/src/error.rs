use std::io;

#[derive(Debug, thiserror::Error)]
pub enum GuestError {
    #[error("PODBOX_CONTAINER (or PODMGR_CONTAINER) environment variable not set")]
    ContainerNameMissing,

    #[error("socket error: {0}")]
    SocketError(#[from] io::Error),

    #[error("I/O error: {0}")]
    IO(io::Error),

    #[error("protocol error: {0}")]
    ProtocolError(#[from] serde_json::Error),

    #[error("handshake failed: {0}")]
    HandshakeFailed(String),
}
