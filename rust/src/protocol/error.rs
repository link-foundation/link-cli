//! Errors raised by the LiNo network protocols.

use std::io;
use thiserror::Error;

/// Everything that can go wrong while encoding, decoding or exchanging a
/// LiNo message.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// The underlying transport failed.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// The peer sent bytes that are not a valid message.
    #[error("malformed message: {0}")]
    Malformed(String),

    /// The message text is not valid LiNo.
    #[error("invalid LiNo: {0}")]
    InvalidLino(String),

    /// The message exceeds one of the configured [`DecodeLimits`](super::DecodeLimits).
    #[error("limit exceeded: {0}")]
    LimitExceeded(String),

    /// The document cannot be represented with the chosen options.
    #[error("cannot encode: {0}")]
    Unencodable(String),

    /// The server answered with an `(error: …)` document.
    #[error("server error: {0}")]
    Remote(String),
}

impl ProtocolError {
    pub(crate) fn malformed(message: impl Into<String>) -> Self {
        Self::Malformed(message.into())
    }
}

/// Result alias used throughout [`crate::protocol`].
pub type ProtocolResult<T> = Result<T, ProtocolError>;
