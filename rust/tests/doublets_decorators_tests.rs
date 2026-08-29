//! Coverage for the upstream `doublets` decorator stack wired into
//! [`DoubletsStorage`] for issue #100.
//!
//! `Foundation.Data.Doublets.Cli.Library` opens every C# database as
//! `new UnitedMemoryLinks<TLinkAddress>(file).DecorateWithAutomaticUniquenessAndUsagesResolution()`.
//! `doublets` 0.5.0 ships that exact stack as
//! `DecoratorsExt::with_automatic_uniqueness_and_usages_resolution`, so these
//! tests pin the Rust behaviour to the C# one instead of re-implementing it here.

use anyhow::Result;
use link_cli::decorators::DecoratorsExt;
use link_cli::{DoubletsStorage, LinksStorage, ResolvedFileMappedUnitStore};
use tempfile::TempDir;

fn open_resolved(
    path: &std::path::Path,
) -> Result<DoubletsStorage<u32, ResolvedFileMappedUnitStore<u32>>> {
    Ok(DoubletsStorage::<u32, _>::open(path)?.with_automatic_uniqueness_and_usages_resolution())
}

#[test]
fn undecorated_storage_still_allows_duplicate_pairs() -> Result<()> {
    let dir = TempDir::new()?;
    let mut storage = DoubletsStorage::<u32, _>::open(dir.path().join("links.doublets"))?;

    let point = storage.create_link(0, 0)?;
    let first = storage.create_link(point, point)?;
    let second = storage.create_link(point, point)?;

    assert_ne!(
        first, second,
        "the bare unit store has no uniqueness policy of its own"
    );
    Ok(())
}

#[test]
fn resolved_storage_turns_duplicate_creation_into_get_or_create() -> Result<()> {
    let dir = TempDir::new()?;
    let mut storage = open_resolved(&dir.path().join("links.doublets"))?;

    let point = storage.create_link(0, 0)?;
    let first = storage.create_link(point, point)?;
    let second = storage.create_link(point, point)?;

    assert_eq!(
        first, second,
        "creating an existing (source, target) pair must resolve to the existing link"
    );
    assert_eq!(storage.search_link(point, point), Some(first));
    assert_eq!(
        storage.links_count(),
        2,
        "the redundant link must be deleted, leaving only the point and the survivor"
    );
    Ok(())
}

#[test]
fn resolved_storage_repoints_usages_of_the_redundant_link() -> Result<()> {
    let dir = TempDir::new()?;
    let mut storage = open_resolved(&dir.path().join("links.doublets"))?;

    let a = storage.create_link(0, 0)?;
    let b = storage.create_link(0, 0)?;
    let survivor = storage.create_link(a, a)?;
    let redundant = storage.create_link(a, b)?;
    let usage = storage.create_link(redundant, redundant)?;

    // Updating `redundant` onto the `(a, a)` pair collides with `survivor`.
    storage.update_link(redundant, a, a)?;

    assert!(
        !storage.link_exists(redundant),
        "the redundant link must be deleted once its usages are migrated"
    );
    assert_eq!(
        storage
            .get_link(usage)
            .map(|link| (link.source, link.target)),
        Some((survivor, survivor)),
        "every usage of the redundant link must be re-pointed at the survivor"
    );
    Ok(())
}

#[test]
fn resolved_storage_cascades_deletion_to_usages() -> Result<()> {
    let dir = TempDir::new()?;
    let mut storage = open_resolved(&dir.path().join("links.doublets"))?;

    let a = storage.create_link(0, 0)?;
    let b = storage.create_link(a, a)?;
    let usage = storage.create_link(b, b)?;

    storage.delete_link(b)?;

    assert!(!storage.link_exists(b));
    assert!(
        !storage.link_exists(usage),
        "deleting a link must cascade to the links that reference it"
    );
    assert!(storage.link_exists(a));
    Ok(())
}

#[test]
fn map_store_preserves_the_backing_path_and_durability() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("links.doublets");

    let mut storage = open_resolved(&path)?;
    assert_eq!(storage.path(), Some(path.as_path()));

    let point = storage.create_link(0, 0)?;
    storage.flush()?;
    drop(storage);

    let reopened = open_resolved(&path)?;
    assert!(reopened.link_exists(point));
    Ok(())
}

#[test]
fn map_store_accepts_any_upstream_decorator() -> Result<()> {
    let dir = TempDir::new()?;
    let mut storage = DoubletsStorage::<u32, _>::open(dir.path().join("links.doublets"))?
        .map_store(DecoratorsExt::with_inner_reference_existence_validation);

    let point = storage.create_link(0, 0)?;
    assert!(storage.create_link(point, point).is_ok());
    assert!(
        storage.create_link(point, 4321).is_err(),
        "the existence validator must reject references to links that do not exist"
    );
    Ok(())
}
