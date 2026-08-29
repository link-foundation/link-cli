//! Link CLI Library - Core functionality for links manipulation
//!
//! This library provides the core data structures and functionality
//! for the link-cli tool, implementing a doublet storage system
//! with LiNo notation support.
//!
//! # Modules
//!
//! Every module is public: `clink` is one front end over this library, and an
//! application that needs a different one should be able to reach the same
//! pieces rather than reimplement them.
//!
//! - `link` - The core Link data structure
//! - `error` - Error types for link operations
//! - `lino_link` - LiNo link representation
//! - `parser` - LiNo notation parser
//! - `link_storage` - Persistent link storage
//! - `storage` - Reusable storage traits and the doublets-backed store
//! - `changes_simplifier` - Changes simplification
//! - `query_processor` - LiNo query processing
//! - `query_types` - Patterns and resolved links, the shapes a query passes through
//! - `link_reference_validator` - Checking, and optionally creating, referenced links
//! - `named_type_links` - The storage interface every layer is written against
//! - `named_types`, `pinned_types` - Named and pinned type decorators
//! - `persistent_transformations` - Persistent transformation triggers
//! - `transactions`, `version_control` - Reversible transitions, branches and tags
//! - `cli` - Argument parsing, so a custom tool can accept the same options
//!
//! # Extending
//!
//! [`NamedTypeLinks`] is the seam: every layer of the stack is written against
//! it, and every decorator both implements it and wraps another implementation
//! of it. A layer of your own -- a cache, an access check, a remote store --
//! implements the trait and slots in anywhere, including under
//! [`QueryProcessor`], which never learns what is beneath it.

pub mod changes_simplifier;
pub mod cli;
pub mod error;
pub mod hybrid_reference;
pub mod link;
pub mod link_reference_validator;
pub mod link_storage;
pub mod link_storage_doublets;
pub mod lino_database_input;
pub mod lino_link;
pub mod named_links;
pub mod named_type_links;
pub mod named_types;
pub mod parser;
pub mod persistent_transformations;
pub mod pinned_types;
pub mod query_options;
pub mod query_processor;
pub mod query_processor_substitution;
pub mod query_types;
pub mod sequences;
pub mod storage;
pub mod transactions;
pub mod unicode_string_storage;
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
pub use link_storage_doublets::link_storage_constants;
pub use lino_database_input::{import_lino_file, import_lino_text};
pub use lino_link::LinoLink;
pub use named_links::NamedLinks;
pub use named_type_links::NamedTypeLinks;
pub use named_types::{NamedTypes, NamedTypesDecorator};
pub use parser::Parser;
pub use persistent_transformations::{
    make_triggers_database_filename, PersistentTransformation, PersistentTransformationDecorator,
    PersistentTransformationKind, PersistentTransformationQuery, TriggerStore,
    INTERNAL_NAME_PREFIX,
};
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
