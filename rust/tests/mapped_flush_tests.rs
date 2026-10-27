//! The selected mapped owner must outlive each flush, including after remaps.
use anyhow::Result;
use doublets::mem::RawMem;
use doublets::unit::{LinkPart, Store};
use link_cli::{DoubletsStorage, LinksStorage, PersistentFileMapped};

#[test]
fn raw_mapping_flushes_its_owned_file_and_rejects_a_closed_view() -> Result<()> {
    let file = tempfile::tempfile()?;
    let reader = file.try_clone()?;
    let mut mapped = PersistentFileMapped::<u64>::new(file)?;
    let flush = mapped.flush_handle();
    assert!(mapped.allocated().is_empty());
    flush.flush()?;
    mapped.grow_filled(32, 0)?[0] = 0x1234_5678_90ab_cdef;
    flush.flush()?;
    let mut reader = reader;
    use std::io::{Read, Seek, SeekFrom};
    reader.seek(SeekFrom::Start(0))?;
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes)?;
    assert_eq!(u64::from_ne_bytes(bytes), mapped.allocated()[0]);
    drop(mapped);
    assert_eq!(
        flush.flush().unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    Ok(())
}

#[test]
fn retained_flush_handle_tracks_growth_shrink_and_existing_capacity() -> Result<()> {
    let file = tempfile::tempfile()?;
    let reopen = file.try_clone()?;
    let mut mapped = PersistentFileMapped::<u64>::new(file)?;
    let flush = mapped.flush_handle();
    mapped.grow_filled(32, 0)?[0] = 73;
    assert_eq!(mapped.grow_filled(8192, 9)?.len(), 8192);
    let last = mapped.allocated().len() - 1;
    mapped.allocated_mut()[last] = 91;
    flush.flush()?;
    mapped.shrink(12)?;
    assert_eq!(mapped.allocated()[0], 73);
    flush.flush()?;
    let capacity = mapped.allocated().len();
    drop(mapped);
    let reopened = PersistentFileMapped::<u64>::open_existing(reopen)?;
    assert_eq!(reopened.allocated().len(), capacity);
    assert_eq!(reopened.allocated()[0], 73);
    reopened.flush_handle().flush()?;
    Ok(())
}

#[test]
fn adopted_mapping_preserves_flush_ownership_through_decorators() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("adopted.doublets");
    let mapped = PersistentFileMapped::<LinkPart<u32>>::from_path(&path)?;
    let flush = mapped.flush_handle();
    let store = Store::<u32, _>::new(mapped)?;
    let mut storage = DoubletsStorage::wrap_at(store, &path)?
        .with_mapped_flush(flush.clone())
        .with_automatic_uniqueness_and_usages_resolution();
    let id = storage.create_link(0, 0)?;
    storage.flush()?;
    storage.update_link(id, id, id)?;
    storage.flush()?;
    drop(storage);
    assert_eq!(
        flush.flush().unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    let reopened = DoubletsStorage::<u32, _>::open(&path)?;
    assert_eq!(
        reopened.get_link(id).map(|row| (row.source, row.target)),
        Some((id, id))
    );
    Ok(())
}

#[test]
fn rejected_overflow_preserves_retained_handles_without_an_allocation_probe() -> Result<()> {
    let mut mapped = PersistentFileMapped::<u64>::new(tempfile::tempfile()?)?;
    mapped.grow_filled(32, 0)?[0] = 42;
    let flush = mapped.flush_handle();
    flush.flush()?;
    // Existing nonzero capacity plus MAX overflows before upstream can unmap
    // or allocate. No deliberately large mapping or memory allocation occurs.
    assert!(mapped.grow_filled(usize::MAX, 0).is_err());
    flush.flush()?;
    assert_eq!(mapped.allocated()[0], 42);
    drop(mapped);
    assert_eq!(
        flush.flush().unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    Ok(())
}

#[test]
fn adopted_owner_without_a_path_still_has_a_checked_barrier() -> Result<()> {
    let file = tempfile::tempfile()?;
    let mapped = PersistentFileMapped::<LinkPart<u32>>::new(file)?;
    let flush = mapped.flush_handle();
    let store = Store::<u32, _>::new(mapped)?;
    let mut storage = DoubletsStorage::wrap(store).with_mapped_flush(flush);
    assert_eq!(storage.path(), None);
    storage.create_link(0, 0)?;
    storage.flush()?;
    Ok(())
}

#[test]
fn replacing_a_mapped_owner_rejects_its_retired_barrier_before_revision_publication() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("replaced.doublets");
    let mut storage = DoubletsStorage::<u32, _>::open(&path)?;
    let index = storage.create_link(0, 0)?;
    storage.flush()?;
    let replacement = PersistentFileMapped::<LinkPart<u32>>::from_path(&path)?;
    let replacement_barrier = replacement.flush_handle();
    let replacement = Store::<u32, _>::new(replacement)?;
    let mut storage = storage.map_store(|previous| {
        drop(previous);
        replacement
    });
    let file = std::fs::File::options().write(true).open(&path)?;
    file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(978_307_200))?;
    let before = file.metadata()?.modified()?;
    assert!(storage.has_external_changes()?);
    let error = storage.flush().unwrap_err();
    assert!(error.to_string().contains("mapped flush owner is closed"));
    assert_eq!(file.metadata()?.modified()?, before);
    assert!(storage.has_external_changes()?);
    let mut storage = storage.with_mapped_flush(replacement_barrier);
    storage.flush()?;
    assert!(!storage.has_external_changes()?);
    storage.update_link(index, index, index)?;
    storage.flush()?;
    drop(storage);
    let reopened = DoubletsStorage::<u32, _>::open(&path)?;
    assert_eq!(
        reopened.get_link(index).map(|row| (row.source, row.target)),
        Some((index, index))
    );
    Ok(())
}
