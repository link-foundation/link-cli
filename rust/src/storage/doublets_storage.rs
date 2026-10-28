//! A [`LinksStorage`] backed by a real `doublets` store.
//!
//! This is the doublets-backed storage the transactions layer composes
//! over. Two shapes are supported:
//!
//! * [`DoubletsStorage::open`] and friends create a **file-mapped**
//!   `doublets::unit::Store` — links live in a memory-mapped file and are
//!   mutated *in place*, so the inode never changes and other processes
//!   that already mapped the same file keep observing the same data.
//! * [`DoubletsStorage::wrap`] adopts a store the caller already owns
//!   (for example a `unit::Store<usize, _>` opened by an embedding
//!   application), which is what makes the transactions layer reusable
//!   without giving up ownership of the database.
//!
//! # Durability
//!
//! Writes go straight into the shared memory mapping, which *is* the
//! page cache on Linux. [`LinksStorage::flush`] synchronizes that file;
//! on Windows, owned mappings first flush the mapped view. Both barriers
//! propagate errors. Drop attempts the same ordering without reporting errors.
//! This does not repair torn pages or make in-place transactions atomic.
//!
//! # Multi-process access
//!
//! A `doublets` store has no internal concurrency control, so concurrent
//! writers to one file will corrupt it. Use [`DoubletsStorage::open_exclusive`]
//! (single writer), [`DoubletsStorage::open_shared`] (concurrent readers)
//! or the [`FileLock`] guards returned by [`DoubletsStorage::lock_shared`] /
//! [`DoubletsStorage::lock_exclusive`] to serialise access, and
//! [`LinksStorage::has_external_changes`] to find out cheaply whether
//! somebody else has written since the last local write.

use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use doublets::data::{Flow, LinkReference};
use doublets::decorators::{AutomaticUniquenessAndUsagesResolution, DecoratorsExt};
use doublets::unit::{LinkPart, Store as UnitStore};
use doublets::Doublets;

use crate::error::LinkError;
use crate::link::GenericLink;
use crate::storage::file_mem::PersistentFileMapped;
use crate::storage::lock::{lock_file_path, FileLock, LockMode};
use crate::storage::traits::{LinksStorage, StorageRevision};
use crate::storage::MappedFlush;

/// The file-mapped `doublets` store used by [`DoubletsStorage::open`].
pub type FileMappedUnitStore<T> = UnitStore<T, PersistentFileMapped<LinkPart<T>>>;

/// The file-mapped store wrapped in the upstream decorator stack that C#
/// applies by default, produced by
/// [`DoubletsStorage::with_automatic_uniqueness_and_usages_resolution`].
///
/// This is the Rust spelling of the C# type produced by
/// `ILinksExtensions.DecorateWithAutomaticUniquenessAndUsagesResolution`,
/// which `Foundation.Data.Doublets.Cli.Library` applies to every
/// `UnitedMemoryLinks<TLinkAddress>` it opens.
pub type ResolvedFileMappedUnitStore<T> =
    AutomaticUniquenessAndUsagesResolution<T, FileMappedUnitStore<T>>;

/// A [`LinksStorage`] over any `doublets` store.
pub struct DoubletsStorage<T: LinkReference, S: Doublets<T>> {
    store: S,
    path: Option<PathBuf>,
    known_revision: StorageRevision,
    lock: Option<FileLock>,
    mapped_flush: Option<MappedFlush>,
    address: PhantomData<T>,
}

impl<T: LinkReference> DoubletsStorage<T, FileMappedUnitStore<T>> {
    /// Opens (or creates) a file-mapped doublets database at `path`
    /// without taking an advisory lock.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, LinkError> {
        Self::open_internal(path, None)
    }

    /// Opens the database and holds a **shared** advisory lock for the
    /// lifetime of the returned storage, excluding concurrent writers.
    pub fn open_shared<P: AsRef<Path>>(path: P) -> Result<Self, LinkError> {
        Self::open_internal(path, Some(LockMode::Shared))
    }

    /// Opens the database and holds an **exclusive** advisory lock for
    /// the lifetime of the returned storage, excluding every other
    /// reader and writer that honours the same protocol.
    pub fn open_exclusive<P: AsRef<Path>>(path: P) -> Result<Self, LinkError> {
        Self::open_internal(path, Some(LockMode::Exclusive))
    }

    /// Like [`Self::open_exclusive`] but returns `Ok(None)` instead of
    /// blocking when another holder owns a conflicting lock.
    pub fn try_open_exclusive<P: AsRef<Path>>(path: P) -> Result<Option<Self>, LinkError> {
        let path = path.as_ref();
        match FileLock::try_acquire(lock_file_path(path), LockMode::Exclusive)? {
            Some(lock) => Ok(Some(Self::open_mapped(path, Some(lock))?)),
            None => Ok(None),
        }
    }

    fn open_internal<P: AsRef<Path>>(path: P, mode: Option<LockMode>) -> Result<Self, LinkError> {
        let path = path.as_ref();
        let lock = match mode {
            Some(mode) => Some(FileLock::acquire(lock_file_path(path), mode)?),
            None => None,
        };
        Self::open_mapped(path, lock)
    }

    fn open_mapped(path: &Path, lock: Option<FileLock>) -> Result<Self, LinkError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mem = PersistentFileMapped::<LinkPart<T>>::from_path(path)?;
        let mapped_flush = Some(mem.flush_handle());
        let store = FileMappedUnitStore::<T>::new(mem)?;
        Ok(Self {
            store,
            path: Some(path.to_path_buf()),
            known_revision: StorageRevision::of(path)?,
            lock,
            mapped_flush,
            address: PhantomData,
        })
    }
}

impl<T: LinkReference, S: Doublets<T>> DoubletsStorage<T, S> {
    /// Adopts a doublets store the caller already owns.
    ///
    /// Nothing about the store is assumed: no path, no locking and no
    /// external-change detection. This is the entry point for embedding
    /// applications that open their own `unit::Store<usize, _>` and only
    /// want the transactions layer on top of it.
    pub fn wrap(store: S) -> Self {
        Self {
            store,
            path: None,
            known_revision: StorageRevision::default(),
            lock: None,
            mapped_flush: None,
            address: PhantomData,
        }
    }

    /// Adopts a store the caller already owns while recording the path
    /// it is backed by, enabling file synchronization,
    /// [`LinksStorage::has_external_changes`] and the lock helpers.
    ///
    /// For an adopted mapping, attach its matching view barrier with
    /// [`Self::with_mapped_flush`]; a path alone cannot discover that view.
    pub fn wrap_at<P: AsRef<Path>>(store: S, path: P) -> Result<Self, LinkError> {
        let path = path.as_ref().to_path_buf();
        let known_revision = StorageRevision::of(&path)?;
        Ok(Self {
            store,
            path: Some(path),
            known_revision,
            lock: None,
            mapped_flush: None,
            address: PhantomData,
        })
    }

    /// Attaches the flush barrier belonging to this store's mapped owner.
    ///
    /// Obtain it from [`PersistentFileMapped::flush_handle`] before moving
    /// that mapping into the store. The handle must belong to this store's
    /// backing file. It is preserved through [`Self::map_store`] and works
    /// even when the adopted store has no recorded path. Arbitrary foreign
    /// stores must provide their own view synchronization.
    pub fn with_mapped_flush(mut self, flush: MappedFlush) -> Self {
        self.mapped_flush = Some(flush);
        self
    }

    /// Replaces the underlying store with `map(store)`, keeping the path,
    /// advisory lock, mapped flush owner and change-detection fingerprint.
    ///
    /// This is the extension point for stacking any `doublets` decorator
    /// (or a custom one) under the transactions and version control
    /// layers. If the closure replaces the mapped owner instead of retaining
    /// it, attach that replacement's barrier with [`Self::with_mapped_flush`].
    /// A retired owner's barrier rejects flush rather than publishing changes.
    ///
    /// ```no_run
    /// use doublets::decorators::DecoratorsExt;
    /// use link_cli::storage::DoubletsStorage;
    ///
    /// # fn main() -> Result<(), link_cli::LinkError> {
    /// let storage = DoubletsStorage::<u32, _>::open("links.data")?
    ///     .map_store(|store| store.with_inner_reference_existence_validation());
    /// # Ok(()) }
    /// ```
    pub fn map_store<S2, F>(self, map: F) -> DoubletsStorage<T, S2>
    where
        S2: Doublets<T>,
        F: FnOnce(S) -> S2,
    {
        DoubletsStorage {
            store: map(self.store),
            path: self.path,
            known_revision: self.known_revision,
            lock: self.lock,
            mapped_flush: self.mapped_flush,
            address: PhantomData,
        }
    }

    /// Wraps the underlying store in the same decorator stack C# applies
    /// through `ILinksExtensions.DecorateWithAutomaticUniquenessAndUsagesResolution`.
    ///
    /// After this call `(source, target)` pairs are unique: creating or
    /// updating a link into a pair that already exists resolves to the
    /// existing link, re-points every usage of the redundant link at the
    /// survivor and deletes the redundant link. Deleting a link cascades
    /// to its usages and resets its contents first.
    ///
    /// ```no_run
    /// use link_cli::storage::{DoubletsStorage, LinksStorage};
    ///
    /// # fn main() -> Result<(), link_cli::LinkError> {
    /// let mut storage = DoubletsStorage::<u32, _>::open("links.data")?
    ///     .with_automatic_uniqueness_and_usages_resolution();
    /// let first = storage.create_link(1, 1)?;
    /// let second = storage.create_link(1, 1)?;
    /// assert_eq!(first, second);
    /// # Ok(()) }
    /// ```
    pub fn with_automatic_uniqueness_and_usages_resolution(
        self,
    ) -> DoubletsStorage<T, AutomaticUniquenessAndUsagesResolution<T, S>> {
        self.map_store(DecoratorsExt::with_automatic_uniqueness_and_usages_resolution)
    }

    /// The database file backing this storage, when known.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Borrows the underlying doublets store.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Mutably borrows the underlying doublets store.
    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    /// Returns the underlying doublets store, dropping any held lock.
    pub fn into_store(self) -> S {
        self.store
    }

    /// Acquires a shared advisory lock on this database's sidecar lock file.
    pub fn lock_shared(&self) -> Result<FileLock, LinkError> {
        FileLock::acquire(self.require_lock_path()?, LockMode::Shared)
    }

    /// Acquires an exclusive advisory lock on this database's sidecar lock file.
    pub fn lock_exclusive(&self) -> Result<FileLock, LinkError> {
        FileLock::acquire(self.require_lock_path()?, LockMode::Exclusive)
    }

    /// The advisory lock held for the lifetime of this storage, if any.
    pub fn held_lock(&self) -> Option<&FileLock> {
        self.lock.as_ref()
    }

    fn require_lock_path(&self) -> Result<PathBuf, LinkError> {
        self.path.as_ref().map(lock_file_path).ok_or_else(|| {
            LinkError::Lock("this doublets storage is not backed by a known file".to_string())
        })
    }

    fn refresh_revision(&mut self) -> Result<(), LinkError> {
        if let Some(path) = self.path.as_ref() {
            self.known_revision = StorageRevision::of(path)?;
        }
        Ok(())
    }
}

impl<T: LinkReference, S: Doublets<T>> LinksStorage<T> for DoubletsStorage<T, S> {
    fn create_link(&mut self, source: T, target: T) -> Result<T, LinkError> {
        Ok(Doublets::create_link(&mut self.store, source, target)?)
    }

    fn create_link_observed(
        &mut self,
        source: T,
        target: T,
        observer: &mut dyn FnMut(GenericLink<T>, GenericLink<T>),
    ) -> Result<T, LinkError> {
        let mut index = T::from_byte(0);
        Doublets::create_link_with(&mut self.store, source, target, |before, after| {
            index = after.index;
            observer(before.into(), after.into());
            Flow::Continue
        })?;
        Ok(index)
    }

    fn ensure_link_created(&mut self, index: T) -> Result<T, LinkError> {
        if self.link_exists(index) {
            return Ok(index);
        }
        // `unit::Store` hands out the lowest free address, reusing the
        // slots of deleted links first, so repeatedly creating empty
        // links walks up to (and reuses) `index`.
        loop {
            let created = Doublets::create(&mut self.store)?;
            match created.cmp(&index) {
                std::cmp::Ordering::Equal => return Ok(index),
                std::cmp::Ordering::Less => continue,
                std::cmp::Ordering::Greater => {
                    return Err(LinkError::StorageError(format!(
                    "could not reserve link address {index}: the store allocated {created} instead"
                )))
                }
            }
        }
    }

    fn get_link(&self, index: T) -> Option<GenericLink<T>> {
        Doublets::get_link(&self.store, index).map(GenericLink::from)
    }

    fn link_exists(&self, index: T) -> bool {
        Doublets::get_link(&self.store, index).is_some()
    }

    fn update_link(&mut self, index: T, source: T, target: T) -> Result<GenericLink<T>, LinkError> {
        let before = self
            .get_link(index)
            .ok_or_else(|| LinkError::not_found(index))?;
        Doublets::update(&mut self.store, index, source, target)?;
        Ok(before)
    }

    fn delete_link(&mut self, index: T) -> Result<GenericLink<T>, LinkError> {
        let before = self
            .get_link(index)
            .ok_or_else(|| LinkError::not_found(index))?;
        Doublets::delete(&mut self.store, index)?;
        Ok(before)
    }

    fn update_link_observed(
        &mut self,
        index: T,
        source: T,
        target: T,
        observer: &mut dyn FnMut(GenericLink<T>, GenericLink<T>),
    ) -> Result<GenericLink<T>, LinkError> {
        let before = self
            .get_link(index)
            .ok_or_else(|| LinkError::not_found(index))?;
        Doublets::update_with(&mut self.store, index, source, target, |before, after| {
            observer(before.into(), after.into());
            Flow::Continue
        })?;
        Ok(before)
    }

    fn delete_link_observed(
        &mut self,
        index: T,
        observer: &mut dyn FnMut(GenericLink<T>, GenericLink<T>),
    ) -> Result<GenericLink<T>, LinkError> {
        let before = self
            .get_link(index)
            .ok_or_else(|| LinkError::not_found(index))?;
        Doublets::delete_with(&mut self.store, index, |before, after| {
            observer(before.into(), after.into());
            Flow::Continue
        })?;
        Ok(before)
    }

    fn all_links(&self) -> Vec<GenericLink<T>> {
        let mut links = Vec::new();
        Doublets::each(&self.store, |link| {
            links.push(GenericLink::from(link));
            Flow::Continue
        });
        links
    }

    fn query_links(
        &self,
        index: Option<T>,
        source: Option<T>,
        target: Option<T>,
    ) -> Vec<GenericLink<T>> {
        let any = self.store.constants().any;
        let zero = T::from_byte(0);
        // Unit-store indexes omit null endpoints, and its concrete-pair lookup
        // returns only one duplicate. Use at most one usable endpoint index,
        // then apply the public trait's literal optional filters to every row.
        let indexed_source = source.filter(|value| *value != zero && *value != any);
        let indexed_target = if indexed_source.is_some() {
            None
        } else {
            target.filter(|value| *value != zero && *value != any)
        };
        let query = [
            index.unwrap_or(any),
            indexed_source.unwrap_or(any),
            indexed_target.unwrap_or(any),
        ];
        let mut links = Vec::new();
        Doublets::each_by(&self.store, query, |link| {
            if index.is_none_or(|value| link.index == value)
                && source.is_none_or(|value| link.source == value)
                && target.is_none_or(|value| link.target == value)
            {
                links.push(GenericLink::from(link));
            }
            Flow::Continue
        });
        links
    }

    fn search_link(&self, source: T, target: T) -> Option<T> {
        let zero = T::from_byte(0);
        let any = self.store.constants().any;
        if source != zero && target != zero && source != any && target != any {
            return Doublets::search(&self.store, source, target);
        }
        // Null endpoints are not indexed; `any` is a literal here, not the
        // upstream query wildcard. Stop at the first exact pair without
        // materializing a complete copy of the store.
        let mut found = None;
        Doublets::each(&self.store, |link| {
            if link.source == source && link.target == target {
                found = Some(link.index);
                Flow::Break
            } else {
                Flow::Continue
            }
        });
        found
    }

    fn get_or_create_link(&mut self, source: T, target: T) -> Result<T, LinkError> {
        match self.search_link(source, target) {
            Some(index) => Ok(index),
            None => self.create_link(source, target),
        }
    }

    fn links_count(&self) -> usize {
        TryInto::<usize>::try_into(Doublets::count(&self.store)).unwrap_or(usize::MAX)
    }

    /// Flushes the owned mapped view, synchronizes its file, and publishes
    /// changes by advancing the known path's modification time. Without an
    /// attached mapped owner, only the known file is synchronized. A no-op
    /// for adopted stores with neither a flush owner nor a known path.
    ///
    /// Bumping the timestamp is deliberate: the kernel only refreshes
    /// `mtime` when a *clean* page of a shared mapping is first written
    /// to, so a long-lived writer that keeps touching already-dirty
    /// pages would otherwise stay invisible to
    /// [`LinksStorage::has_external_changes`].
    fn flush(&mut self) -> Result<(), LinkError> {
        if let Some(flush) = &self.mapped_flush {
            flush.flush()?;
        }
        if let Some(path) = self.path.clone() {
            let file = std::fs::File::options().write(true).open(&path)?;
            if self.mapped_flush.is_none() {
                file.sync_all()?;
            }
            file.set_modified(std::time::SystemTime::now())?;
            self.refresh_revision()?;
        }
        Ok(())
    }

    /// Compares the database file's size and mtime against the values
    /// observed when this storage was opened, reloaded or flushed.
    ///
    /// The granularity is a **published** write: writers publish by
    /// calling [`LinksStorage::flush`], which `fsync`s and advances the
    /// file's modification time. Writes that a peer has made but not yet
    /// flushed are already visible through the shared mapping, but are
    /// not reported here — take [`DoubletsStorage::lock_shared`] when an
    /// exact answer is required.
    fn has_external_changes(&self) -> Result<bool, LinkError> {
        match self.path.as_ref() {
            Some(path) => Ok(StorageRevision::of(path)? != self.known_revision),
            None => Ok(false),
        }
    }

    /// Memory-mapped stores always read through to the mapping, so this
    /// only refreshes the change-detection fingerprint.
    fn reload(&mut self) -> Result<(), LinkError> {
        self.refresh_revision()
    }
}
