//! [`LinksStorage`] implementations for the CLI's in-memory decorators.
//!
//! These make [`LinkStorage`], [`PinnedTypesDecorator`] and
//! [`NamedTypesDecorator`] usable anywhere the generic transactions layer
//! expects a storage, without changing their existing inherent API.
//!
//! # Durability
//!
//! All three keep their links in memory and rewrite the whole database
//! file on [`LinksStorage::flush`], so **`flush` (or the inherent `save`)
//! is required for durability**: nothing reaches the disk before it. The
//! rewrite truncates the existing file in place rather than renaming a
//! temporary over it, so the inode is preserved and other processes that
//! already opened the database keep pointing at the same file.

use doublets::data::LinkReference;

use crate::error::LinkError;
use crate::link::{GenericLink, Link};
use crate::link_storage::LinkStorage;
use crate::named_types::NamedTypesDecorator;
use crate::pinned_types::PinnedTypesDecorator;
use crate::storage::traits::{LinksStorage, LinksStorageRef, StorageRevision};

fn storage_error(error: anyhow::Error) -> LinkError {
    LinkError::StorageError(format!("{error:#}"))
}

/// Generates the delegating [`LinksStorage`]/[`LinksStorageRef`] impls
/// shared by the three in-memory decorators, which expose the same
/// inherent method set.
macro_rules! impl_in_memory_links_storage {
    ($type:ty, $create:expr, $flush:expr, $external:expr, $reload:expr) => {
        impl LinksStorage<u32> for $type {
            fn create_link(&mut self, source: u32, target: u32) -> Result<u32, LinkError> {
                #[allow(clippy::redundant_closure_call)]
                Ok($create(self, source, target))
            }

            fn ensure_link_created(&mut self, index: u32) -> Result<u32, LinkError> {
                Ok(self.ensure_created(index))
            }

            fn get_link(&self, index: u32) -> Option<GenericLink<u32>> {
                self.get(index).copied()
            }

            fn link_exists(&self, index: u32) -> bool {
                self.exists(index)
            }

            fn update_link(
                &mut self,
                index: u32,
                source: u32,
                target: u32,
            ) -> Result<GenericLink<u32>, LinkError> {
                self.update(index, source, target).map_err(storage_error)
            }

            fn delete_link(&mut self, index: u32) -> Result<GenericLink<u32>, LinkError> {
                self.delete(index).map_err(storage_error)
            }

            fn all_links(&self) -> Vec<GenericLink<u32>> {
                self.all().into_iter().copied().collect()
            }

            fn query_links(
                &self,
                index: Option<u32>,
                source: Option<u32>,
                target: Option<u32>,
            ) -> Vec<GenericLink<u32>> {
                self.query(index, source, target)
                    .into_iter()
                    .copied()
                    .collect()
            }

            fn search_link(&self, source: u32, target: u32) -> Option<u32> {
                self.search(source, target)
            }

            fn get_or_create_link(&mut self, source: u32, target: u32) -> Result<u32, LinkError> {
                Ok(self.get_or_create(source, target))
            }

            fn flush(&mut self) -> Result<(), LinkError> {
                #[allow(clippy::redundant_closure_call)]
                $flush(self)
            }

            fn has_external_changes(&self) -> Result<bool, LinkError> {
                #[allow(clippy::redundant_closure_call)]
                $external(self)
            }

            fn reload(&mut self) -> Result<(), LinkError> {
                #[allow(clippy::redundant_closure_call)]
                $reload(self)
            }
        }

        impl LinksStorageRef<u32> for $type {
            fn get_link_ref(&self, index: u32) -> Option<&Link> {
                self.get(index)
            }

            fn all_link_refs(&self) -> Vec<&Link> {
                self.all()
            }

            fn query_link_refs(
                &self,
                index: Option<u32>,
                source: Option<u32>,
                target: Option<u32>,
            ) -> Vec<&Link> {
                self.query(index, source, target)
            }
        }
    };
}

impl_in_memory_links_storage!(
    LinkStorage,
    |storage: &mut LinkStorage, source, target| storage.create(source, target),
    |storage: &mut LinkStorage| {
        storage.save().map_err(storage_error)?;
        storage.refresh_observed_revision()
    },
    |storage: &LinkStorage| {
        Ok(StorageRevision::of(storage.database_path())? != storage.observed_revision())
    },
    |storage: &mut LinkStorage| storage.reload_from_disk().map_err(storage_error)
);

impl_in_memory_links_storage!(
    PinnedTypesDecorator,
    |storage: &mut PinnedTypesDecorator, source, target| storage.create(source, target),
    |storage: &mut PinnedTypesDecorator| {
        storage.save().map_err(storage_error)?;
        storage.links_mut().refresh_observed_revision()
    },
    |storage: &PinnedTypesDecorator| storage.links().has_external_changes(),
    |storage: &mut PinnedTypesDecorator| storage.links_mut().reload()
);

impl_in_memory_links_storage!(
    NamedTypesDecorator,
    |storage: &mut NamedTypesDecorator, source, target| storage.create(source, target),
    |storage: &mut NamedTypesDecorator| {
        storage.save().map_err(storage_error)?;
        storage.links_mut().refresh_observed_revision()?;
        storage.names_links_mut().refresh_observed_revision()
    },
    |storage: &NamedTypesDecorator| {
        Ok(storage.links().has_external_changes()?
            || storage.names_links().has_external_changes()?)
    },
    |storage: &mut NamedTypesDecorator| {
        storage.links_mut().reload()?;
        storage.names_links_mut().reload()
    }
);

/// Compile-time proof that the address type is not hard-coded to `u32`:
/// the generic bound below accepts any `doublets` address type.
#[allow(dead_code)]
fn assert_generic_over_address<T: LinkReference, S: LinksStorage<T>>(_storage: &S) {}
