//! Error types for link operations
//!
//! This module defines all error types used throughout the link-cli.

use thiserror::Error;

/// Error types for link operations
#[derive(Error, Debug)]
pub enum LinkError {
    #[error("Link not found: {0}")]
    NotFound(u32),

    #[error("Invalid link format: {0}")]
    InvalidFormat(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}
