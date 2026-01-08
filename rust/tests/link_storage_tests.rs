//! Tests for the LinkStorage module

use anyhow::Result;
use link_cli::LinkStorage;
use tempfile::NamedTempFile;

#[test]
fn test_storage_create() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let id = storage.create(2, 3);

    assert!(id > 0);
    let link = storage.get(id).unwrap();
    assert_eq!(link.source, 2);
    assert_eq!(link.target, 3);

    Ok(())
}

#[test]
fn test_storage_update() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let id = storage.create(2, 3);
    storage.update(id, 4, 5)?;

    let link = storage.get(id).unwrap();
    assert_eq!(link.source, 4);
    assert_eq!(link.target, 5);

    Ok(())
}

#[test]
fn test_storage_delete() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let id = storage.create(2, 3);
    storage.delete(id)?;

    assert!(storage.get(id).is_none());

    Ok(())
}

#[test]
fn test_storage_persistence() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    // Create and save
    {
        let mut storage = LinkStorage::new(db_path, false)?;
        storage.create(2, 3);
        storage.save()?;
    }

    // Load and verify
    {
        let storage = LinkStorage::new(db_path, false)?;
        let links = storage.all();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].source, 2);
        assert_eq!(links[0].target, 3);
    }

    Ok(())
}

#[test]
fn test_storage_named_links() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let id = storage.get_or_create_named("test");

    assert!(id > 0);
    assert_eq!(storage.get_name(id), Some(&"test".to_string()));
    assert_eq!(storage.get_by_name("test"), Some(id));

    Ok(())
}

#[test]
fn test_storage_search() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let id = storage.create(2, 3);

    assert_eq!(storage.search(2, 3), Some(id));
    assert_eq!(storage.search(1, 1), None);

    Ok(())
}

#[test]
fn test_storage_get_or_create() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;

    let id1 = storage.get_or_create(2, 3);
    let id2 = storage.get_or_create(2, 3);

    assert_eq!(id1, id2);

    Ok(())
}
