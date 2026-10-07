//! Errors raised by the LiNo network protocols.

use links_notation::binary::BinaryError;
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

    /// The message exceeds one of the configured [`ProtocolLimits`](super::ProtocolLimits).
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

/// Each [`BinaryError`] of links-notation becomes the variant of the same name,
/// with the same message.
impl From<BinaryError> for ProtocolError {
    fn from(error: BinaryError) -> Self {
        match error {
            BinaryError::Io(error) => Self::Io(error),
            BinaryError::Malformed(detail) => Self::Malformed(detail),
            BinaryError::InvalidLino(detail) => Self::InvalidLino(detail),
            BinaryError::LimitExceeded(detail) => Self::LimitExceeded(detail),
            BinaryError::Unencodable(detail) => Self::Unencodable(detail),
        }
    }
}

/// Result alias used throughout [`crate::protocol`].
pub type ProtocolResult<T> = Result<T, ProtocolError>;
