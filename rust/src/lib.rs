//! Link CLI Library - Core functionality for links manipulation
//!
//! This library provides the core data structures and functionality
//! for the link-cli tool, implementing a doublet storage system
//! with LiNo notation support.
//!
//! # Modules
//!
//! - `link` - The core Link data structure
//! - `error` - Error types for link operations
//! - `lino_link` - LiNo link representation
//! - `parser` - LiNo notation parser
//! - `link_storage` - Persistent link storage
//! - `changes_simplifier` - Changes simplification
//! - `query_processor` - LiNo query processing

mod changes_simplifier;
mod error;
mod link;
mod link_storage;
mod lino_link;
mod parser;
mod query_processor;

// Re-export main types for easy access
pub use changes_simplifier::simplify_changes;
pub use error::LinkError;
pub use link::Link;
pub use link_storage::LinkStorage;
pub use lino_link::LinoLink;
pub use parser::Parser;
pub use query_processor::{QueryOptions, QueryProcessor};
