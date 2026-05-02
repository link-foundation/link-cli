use anyhow::Result;
use link_cli::{LinkStorage, PinnedTypesAccess, PinnedTypesDecorator};
use tempfile::NamedTempFile;

#[test]
fn decorator_exposes_link_storage_operations_and_pinned_types() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let links = LinkStorage::new(db_path, false)?;
    let mut decorator = PinnedTypesDecorator::from_link_storage(links);

    assert_eq!(vec![1, 2, 3], decorator.pinned_types(3)?);
    assert!(decorator.exists(1));
    assert!(decorator.exists(2));
    assert!(decorator.exists(3));

    let link = decorator.get_or_create(10, 20);

    assert!(decorator.links().exists(link));
    assert_eq!(Some(link), decorator.search(10, 20));

    Ok(())
}

#[test]
fn decorator_supports_triplet_deconstruction_parity() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let links = LinkStorage::new(db_path, false)?;
    let mut decorator = PinnedTypesDecorator::from_link_storage(links);

    let (type_type, unicode_symbol_type, unicode_sequence_type) =
        decorator.deconstruct_pinned_types()?;

    assert_eq!(
        (1, 2, 3),
        (type_type, unicode_symbol_type, unicode_sequence_type)
    );

    Ok(())
}

#[test]
fn decorator_rejects_unexpected_link_shape_at_reserved_address() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let links = LinkStorage::new(db_path, false)?;
    let mut decorator = PinnedTypesDecorator::from_link_storage(links);

    decorator.create(1, 0);
    let error = decorator.pinned_types(1).unwrap_err();

    assert!(error
        .to_string()
        .contains("Unexpected link found at address 1"));

    Ok(())
}
