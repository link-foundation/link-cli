//! Reusable storage layer: traits, a doublets-backed implementation and
//! advisory locking helpers.
//!
//! See [`LinksStorage`] for the abstraction the transactions layer is
//! written against, and [`DoubletsStorage`] for the file-mapped
//! `doublets` implementation.

mod decorator_impls;
mod doublets_storage;
pub mod lock;
mod traits;

pub use doublets_storage::{DoubletsStorage, FileMappedUnitStore};
pub use lock::{lock_file_path, FileLock, LockMode};
pub use traits::{LinksStorage, LinksStorageRef, StorageRevision};
