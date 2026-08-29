//! Reusable storage layer: traits, a doublets-backed implementation and
//! advisory locking helpers.
//!
//! See [`LinksStorage`] for the abstraction the transactions layer is
//! written against, and [`DoubletsStorage`] for the file-mapped
//! `doublets` implementation.

/// The upstream `doublets` decorator layer, re-exported so downstream
/// crates can stack decorators onto a [`DoubletsStorage`] through
/// [`DoubletsStorage::map_store`] without depending on `doublets`
/// directly (and without risking a version mismatch).
pub use doublets::decorators;

mod decorator_impls;
mod doublets_storage;
mod file_mem;
pub mod lock;
mod traits;

pub use doublets_storage::{DoubletsStorage, FileMappedUnitStore, ResolvedFileMappedUnitStore};
pub use file_mem::PersistentFileMapped;
pub use lock::{lock_file_path, FileLock, LockMode};
pub use traits::{LinksStorage, LinksStorageRef, StorageRevision};
