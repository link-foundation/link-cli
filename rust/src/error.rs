//! Error types for link operations
//!
//! This module defines all error types used throughout the link-cli.
//!
//! [`LinkError`] is the typed error exposed by the public storage and
//! transactions API. It is deliberately **not** `anyhow::Error`, so
//! external crates embedding this library can match on failures instead
//! of inspecting strings. Because `LinkError` implements
//! [`std::error::Error`], every `LinkError` still converts into
//! `anyhow::Error` with `?` for callers that prefer `anyhow`.

use doublets::data::LinkReference;
use thiserror::Error;

/// Error types for link operations
#[derive(Error, Debug)]
pub enum LinkError {
    /// No link exists at the requested address.
    ///
    /// The address is widened to `u128` so the same error type can be
    /// used with every `doublets` address type (`u32`, `u64`, `usize`).
    #[error("Link not found: {0}")]
    NotFound(u128),

    #[error("Invalid link format: {0}")]
    InvalidFormat(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    /// Filesystem failure while reading or writing a links database.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Failure reported by the underlying `doublets` store.
    #[error("Doublets store error: {0}")]
    Doublets(String),

    /// Advisory file lock could not be acquired or released.
    #[error("Lock error: {0}")]
    Lock(String),

    /// Invalid use of the transactions layer (nested transaction, ...).
    #[error("Transaction error: {0}")]
    Transaction(String),

    /// A recorded address does not fit into the configured address type.
    #[error("Address {0} does not fit into the configured link address type")]
    AddressOutOfRange(u128),
}

impl LinkError {
    /// Builds a [`LinkError::NotFound`] from any `doublets` address type.
    pub fn not_found<T: LinkReference>(index: T) -> Self {
        Self::NotFound(to_u128(index))
    }
}

impl<T: LinkReference> From<doublets::Error<T>> for LinkError {
    fn from(error: doublets::Error<T>) -> Self {
        match error {
            doublets::Error::NotExists(index) => Self::NotFound(to_u128(index)),
            other => Self::Doublets(other.to_string()),
        }
    }
}

/// Widens any `doublets` address into `u128` for error reporting.
///
/// `LinkReference: TryInto<u128, Error: Debug>` and every supported
/// address type is unsigned and at most 128 bits wide, so this never
/// actually fails; the fallback keeps the helper total.
fn to_u128<T: LinkReference>(index: T) -> u128 {
    TryInto::<u128>::try_into(index).unwrap_or(u128::MAX)
}
