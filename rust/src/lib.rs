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
pub mod cli;
mod error;
mod hybrid_reference;
mod link;
mod link_storage;
mod lino_link;
mod named_links;
mod named_types;
mod parser;
mod pinned_types;
mod query_processor;
pub mod sequences;
mod unicode_string_storage;

// Re-export main types for easy access
pub use changes_simplifier::simplify_changes;
pub use error::LinkError;
pub use hybrid_reference::{external_reference, external_reference_value, HybridReference};
pub use link::{DoubletsLink, Link};
pub use link_storage::LinkStorage;
pub use lino_link::LinoLink;
pub use named_links::NamedLinks;
pub use named_types::{NamedTypes, NamedTypesDecorator};
pub use parser::Parser;
pub use pinned_types::{PinnedTypes, PinnedTypesAccess, PinnedTypesDecorator};
pub use query_processor::{QueryOptions, QueryProcessor};
pub use unicode_string_storage::UnicodeStringStorage;
