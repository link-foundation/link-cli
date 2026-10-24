//! Backends for semantic contracts shared by native and actual WASM tests.
#![allow(dead_code)] // Each test binary uses only part of these fixture helpers.

use anyhow::Result;
use link_cli::{LinkStorage, NamedTypesDecorator};

#[cfg(not(target_arch = "wasm32"))]
pub type StorageOwner = tempfile::NamedTempFile;
#[cfg(target_arch = "wasm32")]
pub type StorageOwner = ();
pub type NamedFiles = (StorageOwner, StorageOwner);

/// Return the owner first so a destructured store is dropped before its file.
pub fn links() -> Result<(StorageOwner, LinkStorage)> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let file = tempfile::NamedTempFile::new()?;
        let storage = LinkStorage::new(file.path(), false)?;
        Ok((file, storage))
    }
    #[cfg(target_arch = "wasm32")]
    {
        Ok(((), LinkStorage::in_memory(false)))
    }
}

pub fn named() -> Result<(NamedFiles, NamedTypesDecorator)> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let database = tempfile::NamedTempFile::new()?;
        let names = tempfile::NamedTempFile::new()?;
        let storage =
            NamedTypesDecorator::with_names_database_path(database.path(), names.path(), false)?;
        Ok(((database, names), storage))
    }
    #[cfg(target_arch = "wasm32")]
    {
        Ok((
            ((), ()),
            NamedTypesDecorator::from_link_storages(
                LinkStorage::in_memory(false),
                LinkStorage::in_memory(false),
            ),
        ))
    }
}
