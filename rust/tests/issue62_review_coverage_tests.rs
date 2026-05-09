use anyhow::Result;
use link_cli::{LinkStorage, NamedTypeLinks, PinnedTypes, QueryProcessor};
use tempfile::NamedTempFile;

#[test]
fn pinned_types_take_types_is_finite_and_deterministic() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let mut storage = LinkStorage::new(db_path, false)?;
    let mut pinned_types = PinnedTypes::new(&mut storage);

    assert_eq!(Vec::<u32>::new(), pinned_types.take_types(0)?);
    assert_eq!(vec![1, 2, 3, 4, 5, 6], pinned_types.take_types(6)?);

    Ok(())
}

#[test]
fn unsupported_any_reference_is_rejected_without_placeholder_creation() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let mut storage = LinkStorage::new(db_path, false)?;

    let error = storage.try_ensure_created(u32::MAX).unwrap_err();

    assert!(error
        .to_string()
        .contains("Cannot ensure unsupported link address"));
    assert!(storage.all().is_empty());

    Ok(())
}

#[test]
fn explicit_numeric_id_update_can_be_reversed_with_another_update() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let mut storage = LinkStorage::new(db_path, false)?;
    let processor = QueryProcessor::new(false).with_auto_create_missing_references(true);

    processor.process_query(&mut storage, "(() ((1: 1 1)))")?;
    assert_link(&storage, 1, 1, 1);

    processor.process_query(&mut storage, "(((1: 1 1)) ((1: 2 2)))")?;
    assert_link(&storage, 1, 2, 2);

    processor.process_query(&mut storage, "(((1: 2 2)) ((1: 1 1)))")?;
    assert_link(&storage, 1, 1, 1);

    Ok(())
}

#[test]
fn named_link_create_delete_recreate_clears_stale_name_mapping() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let mut storage = LinkStorage::new(db_path, false)?;
    let processor = QueryProcessor::new(false).with_auto_create_missing_references(true);

    processor.process_query(&mut storage, "(() ((child: father mother)))")?;
    let first_child = storage.get_by_name("child").expect("child is named");

    processor.process_query(&mut storage, "((child: father mother)) ()")?;
    assert_eq!(None, storage.get_by_name("child"));
    assert_eq!(None, storage.get_name(first_child));

    processor.process_query(&mut storage, "(() ((child: father mother)))")?;
    let recreated_child = storage.get_by_name("child").expect("child is recreated");
    assert_eq!(
        Some("child"),
        storage.get_name(recreated_child).map(String::as_str)
    );

    Ok(())
}

fn assert_link(storage: &LinkStorage, index: u32, source: u32, target: u32) {
    let link = storage.get(index).expect("link should exist");
    assert_eq!(index, link.index);
    assert_eq!(source, link.source);
    assert_eq!(target, link.target);
}
