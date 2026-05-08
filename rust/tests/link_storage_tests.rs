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

#[test]
fn test_lino_lines_use_numbered_references_without_names() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    storage.create(1, 1);
    storage.create(1, 2);

    assert_eq!(storage.lino_lines(), vec!["(1: 1 1)", "(2: 1 2)"]);

    Ok(())
}

#[test]
fn test_lino_lines_use_names_for_indexes_sources_and_targets() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let father = storage.get_or_create_named("father");
    let mother = storage.get_or_create_named("mother");
    let child = storage.create(father, mother);
    storage.set_name(child, "child");

    assert_eq!(
        storage.lino_lines(),
        vec![
            "(father: father father)",
            "(mother: mother mother)",
            "(child: father mother)"
        ]
    );

    Ok(())
}

#[test]
fn test_lino_lines_escape_names_that_need_quoting() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let source = storage.create(1, 1);
    storage.set_name(source, "source name");
    let target = storage.create(2, 2);
    storage.set_name(target, "target:ref");
    let child = storage.create(source, target);
    storage.set_name(child, "child(ref)");

    assert_eq!(
        storage.lino_lines(),
        vec![
            "('source name': 'source name' 'source name')",
            "('target:ref': 'target:ref' 'target:ref')",
            "('child(ref)': 'source name' 'target:ref')"
        ]
    );

    Ok(())
}

#[test]
fn test_lino_lines_select_quote_style_for_names_containing_quotes() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let single_quote = storage.create(1, 1);
    storage.set_name(single_quote, "single'quote");
    let double_quote = storage.create(2, 2);
    storage.set_name(double_quote, "double\"quote");
    let both_quotes = storage.create(single_quote, double_quote);
    storage.set_name(both_quotes, "both'\"quote");

    assert_eq!(
        storage.lino_lines(),
        vec![
            "(\"single'quote\": \"single'quote\" \"single'quote\")",
            "('double\"quote': 'double\"quote' 'double\"quote')",
            "('both\\'\"quote': \"single'quote\" 'double\"quote')"
        ]
    );

    Ok(())
}

#[test]
fn test_write_lino_output_writes_complete_database() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let output_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let output_path = output_file.path();

    let mut storage = LinkStorage::new(db_path, false)?;
    storage.create(1, 1);
    storage.create(2, 2);

    storage.write_lino_output(output_path)?;

    assert_eq!(
        std::fs::read_to_string(output_path)?,
        "(1: 1 1)\n(2: 2 2)\n"
    );

    Ok(())
}
