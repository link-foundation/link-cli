//! Storage abstraction shared by every links backend.
//!
//! [`LinksStorage`] is the trait the transactions layer is written
//! against, so any doublets-compatible store — the CLI's own
//! [`LinkStorage`](crate::LinkStorage), a file-mapped
//! [`DoubletsStorage`](super::DoubletsStorage), or an externally owned
//! `doublets::unit::Store` wrapped with
//! [`DoubletsStorage::wrap`](super::DoubletsStorage::wrap) — can be
//! composed under [`GenericTransactionsDecorator`](crate::transactions::GenericTransactionsDecorator).
//!
//! The trait is generic over the doublets address type `T`, so external
//! consumers using `usize`-addressed stores are first-class.

use std::fs;
use std::path::Path;

use doublets::data::LinkReference;

use crate::error::LinkError;
use crate::link::GenericLink;

/// Cheap fingerprint of a database file, used to answer
/// "has anyone else written since I last looked?" without reparsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StorageRevision {
    /// Size of the database file in bytes (0 when it does not exist).
    pub len: u64,
    /// Last modification time in nanoseconds since the Unix epoch.
    pub modified_nanos: u128,
}

impl StorageRevision {
    /// Reads the current revision of `path`.
    ///
    /// A missing file is reported as [`StorageRevision::default`] rather
    /// than an error, so callers can fingerprint a database before it
    /// has been created.
    pub fn of<P: AsRef<Path>>(path: P) -> Result<Self, LinkError> {
        let metadata = match fs::metadata(path.as_ref()) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => return Err(LinkError::Io(error)),
        };
        let modified_nanos = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        Ok(Self {
            len: metadata.len(),
            modified_nanos,
        })
    }
}

/// A store of links addressed by `T`, expressed in terms of owned
/// [`GenericLink<T>`] values so that both in-memory maps and
/// memory-mapped doublets stores can implement it.
pub trait LinksStorage<T: LinkReference> {
    /// Creates a new link and returns its address.
    fn create_link(&mut self, source: T, target: T) -> Result<T, LinkError>;

    /// Ensures a link exists at `index`, creating placeholders as needed.
    fn ensure_link_created(&mut self, index: T) -> Result<T, LinkError>;

    /// Returns the link stored at `index`, if any.
    fn get_link(&self, index: T) -> Option<GenericLink<T>>;

    /// Returns `true` when a link exists at `index`.
    fn link_exists(&self, index: T) -> bool {
        self.get_link(index).is_some()
    }

    /// Repoints `index` at `source`/`target`, returning the previous state.
    fn update_link(&mut self, index: T, source: T, target: T) -> Result<GenericLink<T>, LinkError>;

    /// Deletes `index`, returning the link that was removed.
    fn delete_link(&mut self, index: T) -> Result<GenericLink<T>, LinkError>;

    /// Returns every link in the store.
    fn all_links(&self) -> Vec<GenericLink<T>>;

    /// Returns every link matching the (optional) index/source/target pattern.
    fn query_links(
        &self,
        index: Option<T>,
        source: Option<T>,
        target: Option<T>,
    ) -> Vec<GenericLink<T>> {
        self.all_links()
            .into_iter()
            .filter(|link| {
                index.is_none_or(|value| value == link.index)
                    && source.is_none_or(|value| value == link.source)
                    && target.is_none_or(|value| value == link.target)
            })
            .collect()
    }

    /// Finds the address of a link with the given source and target.
    fn search_link(&self, source: T, target: T) -> Option<T> {
        self.all_links()
            .into_iter()
            .find(|link| link.source == source && link.target == target)
            .map(|link| link.index)
    }

    /// Returns the address of an existing `(source, target)` link,
    /// creating it when it does not exist yet.
    fn get_or_create_link(&mut self, source: T, target: T) -> Result<T, LinkError> {
        match self.search_link(source, target) {
            Some(index) => Ok(index),
            None => self.create_link(source, target),
        }
    }

    /// Number of links currently stored.
    fn links_count(&self) -> usize {
        self.all_links().len()
    }

    /// Makes every write durable on disk.
    ///
    /// Implementations that keep state in memory write it out here;
    /// memory-mapped implementations `fsync` the backing file. Callers
    /// that need crash-consistency guarantees must call this — see the
    /// durability notes on each implementation.
    fn flush(&mut self) -> Result<(), LinkError>;

    /// Cheap check for "did another process write to this database since
    /// we last read or wrote it?".
    ///
    /// The default implementation reports `false`, which is correct for
    /// stores that are not shared through the filesystem.
    fn has_external_changes(&self) -> Result<bool, LinkError> {
        Ok(false)
    }

    /// Re-reads the database from disk, discarding cached state.
    ///
    /// The default implementation is a no-op for stores that always read
    /// through to their backing storage.
    fn reload(&mut self) -> Result<(), LinkError> {
        Ok(())
    }
}

/// Extension implemented by stores that keep links resident in memory and
/// can therefore lend out references instead of copies.
///
/// Memory-mapped stores deliberately do **not** implement this: their
/// links are decoded from raw memory on read, so there is no stable
/// `&GenericLink<T>` to borrow.
pub trait LinksStorageRef<T: LinkReference>: LinksStorage<T> {
    /// Borrows the link stored at `index`.
    fn get_link_ref(&self, index: T) -> Option<&GenericLink<T>>;

    /// Borrows every link in the store.
    fn all_link_refs(&self) -> Vec<&GenericLink<T>>;

    /// Borrows every link matching the (optional) index/source/target pattern.
    fn query_link_refs(
        &self,
        index: Option<T>,
        source: Option<T>,
        target: Option<T>,
    ) -> Vec<&GenericLink<T>> {
        self.all_link_refs()
            .into_iter()
            .filter(|link| {
                index.is_none_or(|value| value == link.index)
                    && source.is_none_or(|value| value == link.source)
                    && target.is_none_or(|value| value == link.target)
            })
            .collect()
    }
}
