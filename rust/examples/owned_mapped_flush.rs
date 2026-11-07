//! Adopt an existing mapping without losing its checked durability barrier.
use doublets::mem::RawMem;
use doublets::unit::{LinkPart, Store};
use link_cli::{DoubletsStorage, LinksStorage, PersistentFileMapped};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("links.data");
    let mapped = PersistentFileMapped::<LinkPart<u32>>::from_path(&path)?;
    let barrier = mapped.flush_handle();
    assert!(mapped.allocated().is_empty());
    let store = Store::<u32, _>::new(mapped)?;
    let mut storage = DoubletsStorage::wrap_at(store, &path)?
        .with_mapped_flush(barrier.clone())
        .with_automatic_uniqueness_and_usages_resolution();
    let index = storage.create_link(0, 0)?;
    storage.update_link(index, index, index)?;
    storage.flush()?; // Windows: mapped view, then file buffers; errors propagate.
    drop(storage);
    assert_eq!(
        barrier.flush().unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    let reopened = DoubletsStorage::<u32, _>::open(&path)?;
    let row = reopened
        .get_link(index)
        .ok_or("persisted link is missing")?;
    assert_eq!((row.source, row.target), (index, index));
    println!("adopted mapping flushed and reopened link {index}; retired handle rejected");
    Ok(())
}
