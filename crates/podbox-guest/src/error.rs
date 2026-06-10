use std::io;

#[derive(Debug, thiserror::Error)]
pub enum GuestError {
    #[error("PODBOX_CONTAINER (or PODMGR_CONTAINER) environment variable not set")]
    ContainerNameMissing,

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("protocol error: {0}")]
    ProtocolError(#[from] serde_json::Error),

    #[error("handshake failed: {0}")]
    HandshakeFailed(String),
}
