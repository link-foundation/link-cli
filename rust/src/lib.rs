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
//! - `permissions` - An optional permission layer with rules stored in links
//! - `undoable_links` - Writes kept to be undone, so a failed request leaves nothing
//! - `types` - A type hierarchy with sub-types and multiple inheritance
//! - `triplets` - Triplets `(index: type from to)` stored in doublets, with substitution and LiNo
//! - `transactions`, `version_control` - Reversible transitions, branches and tags
//! - `union_links` - Many networks seen as one, the first member shadowing the rest
//! - `remap_links` - A network under other addresses: an offset or a table
//! - `external_ids` - The ids of other systems (UUIDs, URLs) bound to links
//! - `graphql` - A Hasura-compatible GraphQL server for the Deep standard
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
mod diagnostics;
pub mod engine;
pub mod error;
pub mod external_ids;
pub mod graphql;
pub mod hybrid_reference;
pub mod indexed_links;
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
pub mod permissions;
pub mod persistent_transformations;
pub mod pinned_types;
pub mod protocol;
pub mod query_address;
pub mod query_options;
mod query_parser;
pub mod query_processor;
pub mod query_processor_substitution;
mod query_reply_budget;
pub mod query_types;
pub mod remap_links;
pub mod sequences;
pub mod storage;
mod structure_format;
mod text_storage_format;
pub mod transactions;
pub mod triplets;
pub mod types;
pub mod undoable_links;
pub mod unicode_string_storage;
pub mod union_links;
pub mod version_control;

/// The `doublets` crate this library is built on, re-exported so
/// downstream crates can name upstream types (stores, decorators,
/// `LinkReference` implementations) without adding their own dependency
/// and risking a version mismatch.
pub use doublets;

// Re-export main types for easy access
pub use changes_simplifier::simplify_changes;
pub use error::LinkError;
pub use external_ids::{LinksExternalIds, EXTERNAL_ID_NAME};
pub use hybrid_reference::{external_reference, external_reference_value, HybridReference};
pub use indexed_links::IndexedLinksDecorator;
pub use link::{DoubletsLink, GenericLink, Link};
pub use link_storage::LinkStorage;
pub use link_storage_doublets::link_storage_constants;
pub use lino_database_input::{import_lino_file, import_lino_text};
pub use lino_link::LinoLink;
pub use named_links::NamedLinks;
pub use named_type_links::NamedTypeLinks;
pub use named_types::{NamedTypes, NamedTypesDecorator};
pub use parser::Parser;
pub use permissions::{
    AccessPolicy, Forbidden, LinksTokenTable, PermissionAccess, PermissionMarkers,
    PermissionPolicy, PermissionsDecorator,
};
pub use persistent_transformations::{
    make_triggers_database_filename, PersistentTransformation, PersistentTransformationDecorator,
    PersistentTransformationKind, PersistentTransformationQuery, TriggerStore,
    INTERNAL_NAME_PREFIX,
};
pub use pinned_types::{PinnedTypes, PinnedTypesAccess, PinnedTypesDecorator};
pub use protocol::{
    BinaryLinoOptions, BinaryLinoProtocol, LinksClient, LinksServer, LinoProtocol, TextLinoProtocol,
};
pub use query_address::QueryAddress;
pub use query_options::QueryOptions;
pub use query_processor::{Changes, GenericQueryProcessor, QueryProcessor};
pub use remap_links::{AddressMap, AddressTable, OffsetAddressMap, RemapLinks};
pub use storage::{
    decorators, lock_file_path, DoubletsStorage, FileLock, FileMappedUnitStore, FileMappedValue,
    LinksStorage, LinksStorageRef, LockMode, MappedFlush, NamedLinksStorage, PersistentFileMapped,
    ResolvedFileMappedUnitStore, StorageRevision,
};
pub use transactions::{
    CommitMode, DoubletLink, FileTransitionLog, GenericDoubletLink, GenericTransactionsDecorator,
    GenericTransition, LogRetentionPolicy, ReplaceableTransitionLogStore, RowUndoPolicy,
    TransactionHandle, TransactionsDecorator, Transition, TransitionKind, TransitionLogStore,
};
pub use triplets::{
    import_triplets_lino, process_triplets_query, triplets_to_lino, DoubletRole, Triplet,
    TripletChange, TripletMarkers, TripletQuery, Triplets, TripletsOverDoublets,
    NOTHING_MARKER_NAME, PAIR_MARKER_NAME,
};
pub use types::{ensure_point, TypeMarkers, Types, TypesDecorator, EXTENDS_NAME, ROOT_TYPE_NAME};
pub use undoable_links::{LinkChange, LinksUndoLog, UndoableLinksDecorator};
pub use unicode_string_storage::UnicodeStringStorage;
pub use union_links::{UnionLinks, UnionMember};
pub use version_control::{
    BranchInfo, GenericVersionControlDecorator, VersionControlDecorator, DEFAULT_BRANCH_NAME,
};
