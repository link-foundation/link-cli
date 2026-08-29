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
//! - `storage` - Reusable storage traits and the doublets-backed store
//! - `changes_simplifier` - Changes simplification
//! - `query_processor` - LiNo query processing

mod changes_simplifier;
pub mod cli;
mod error;
mod hybrid_reference;
mod link;
mod link_reference_validator;
mod link_storage;
mod lino_database_input;
mod lino_link;
mod named_links;
mod named_type_links;
mod named_types;
mod parser;
mod pinned_types;
mod query_options;
mod query_processor;
mod query_processor_substitution;
mod query_types;
pub mod sequences;
pub mod storage;
pub mod transactions;
mod unicode_string_storage;
pub mod version_control;

/// The `doublets` crate this library is built on, re-exported so
/// downstream crates can name upstream types (stores, decorators,
/// `LinkReference` implementations) without adding their own dependency
/// and risking a version mismatch.
pub use doublets;

// Re-export main types for easy access
pub use changes_simplifier::simplify_changes;
pub use error::LinkError;
pub use hybrid_reference::{external_reference, external_reference_value, HybridReference};
pub use link::{DoubletsLink, GenericLink, Link};
pub use link_storage::LinkStorage;
pub use lino_database_input::{import_lino_file, import_lino_text};
pub use lino_link::LinoLink;
pub use named_links::NamedLinks;
pub use named_type_links::NamedTypeLinks;
pub use named_types::{NamedTypes, NamedTypesDecorator};
pub use parser::Parser;
pub use pinned_types::{PinnedTypes, PinnedTypesAccess, PinnedTypesDecorator};
pub use query_options::QueryOptions;
pub use query_processor::QueryProcessor;
pub use storage::{
    decorators, lock_file_path, DoubletsStorage, FileLock, FileMappedUnitStore, LinksStorage,
    LinksStorageRef, LockMode, PersistentFileMapped, ResolvedFileMappedUnitStore, StorageRevision,
};
pub use transactions::{
    CommitMode, DoubletLink, FileTransitionLog, GenericDoubletLink, GenericTransactionsDecorator,
    GenericTransition, LogRetentionPolicy, TransactionHandle, TransactionsDecorator, Transition,
    TransitionKind, TransitionLogStore,
};
pub use unicode_string_storage::UnicodeStringStorage;
pub use version_control::{BranchInfo, VersionControlDecorator, DEFAULT_BRANCH_NAME};
