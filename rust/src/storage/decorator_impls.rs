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
//! rewrite atomically replaces the file, syncs it and (on Unix) its parent
//! directory. Other processes must reload after acquiring their advisory lock.

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
    ($type:ty, $create:expr, $flush:expr, $external:expr, $reload:expr $(, $extra:item)*) => {
        impl LinksStorage<u32> for $type {
            $($extra)*
            fn create_link(&mut self, source: u32, target: u32) -> Result<u32, LinkError> {
                #[allow(clippy::redundant_closure_call)]
                Ok($create(self, source, target))
            }

            fn ensure_link_created(&mut self, index: u32) -> Result<u32, LinkError> {
                Ok(self.ensure_created(index))
            }

            fn ensure_link_created_observed(
                &mut self,
                index: u32,
                observer: &mut dyn FnMut(GenericLink<u32>, GenericLink<u32>),
            ) -> Result<u32, LinkError> {
                let existing = self.exists(index);
                let created = self.ensure_created(index);
                // These adapters remove intermediate allocations; only the
                // requested address survives. Avoid copying every stored row.
                if index != 0 && !existing {
                    observer(GenericLink::null(), *self.get(index).expect("ensured row"));
                }
                Ok(created)
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

            fn update_link_observed(
                &mut self,
                index: u32,
                source: u32,
                target: u32,
                observer: &mut dyn FnMut(GenericLink<u32>, GenericLink<u32>),
            ) -> Result<GenericLink<u32>, LinkError> {
                self.update_observed(index, source, target, observer)
                    .map_err(storage_error)
            }

            fn delete_link_observed(
                &mut self,
                index: u32,
                observer: &mut dyn FnMut(GenericLink<u32>, GenericLink<u32>),
            ) -> Result<GenericLink<u32>, LinkError> {
                self.delete_observed(index, observer).map_err(storage_error)
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
        if !storage.is_in_memory() {
            // FlushFileBuffers on Windows requires a writable handle, even
            // though flushing does not itself modify the file's contents.
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(storage.database_path())?
                .sync_all()?;
            #[cfg(unix)]
            std::fs::File::open(
                storage
                    .database_path()
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| std::path::Path::new(".")),
            )?
            .sync_all()?;
        }
        storage.refresh_observed_revision()
    },
    |storage: &LinkStorage| {
        Ok(!storage.is_in_memory()
            && StorageRevision::of(storage.database_path())? != storage.observed_revision())
    },
    |storage: &mut LinkStorage| storage.reload_from_disk().map_err(storage_error)
);

impl_in_memory_links_storage!(
    PinnedTypesDecorator,
    |storage: &mut PinnedTypesDecorator, source, target| storage.create(source, target),
    |storage: &mut PinnedTypesDecorator| { storage.links_mut().flush() },
    |storage: &PinnedTypesDecorator| storage.links().has_external_changes(),
    |storage: &mut PinnedTypesDecorator| storage.links_mut().reload()
);

impl_in_memory_links_storage!(
    NamedTypesDecorator,
    |storage: &mut NamedTypesDecorator, source, target| storage.create(source, target),
    |storage: &mut NamedTypesDecorator| {
        storage.links_mut().flush()?;
        storage.names_links_mut().flush()
    },
    |storage: &NamedTypesDecorator| {
        Ok(storage.links().has_external_changes()?
            || storage.names_links().has_external_changes()?)
    },
    |storage: &mut NamedTypesDecorator| {
        storage.links_mut().reload()?;
        storage.names_links_mut().reload()
    },
    fn capture_names(
        &mut self,
    ) -> Result<Option<std::collections::BTreeMap<u32, String>>, LinkError> {
        self.logical_names().map(Some).map_err(storage_error)
    },
    fn set_link_name(&mut self, index: u32, name: Option<&str>) -> Result<u32, LinkError> {
        use crate::named_types::NamedTypes;
        match name {
            Some(name) => self.set_name(index, name).map_err(storage_error),
            None => {
                self.remove_name(index).map_err(storage_error)?;
                Ok(0)
            }
        }
    }
);

/// Compile-time proof that the address type is not hard-coded to `u32`:
/// the generic bound below accepts any `doublets` address type.
#[allow(dead_code)]
fn assert_generic_over_address<T: LinkReference, S: LinksStorage<T>>(_storage: &S) {}
