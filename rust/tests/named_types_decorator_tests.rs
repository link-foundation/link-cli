use anyhow::Result;
use link_cli::{
    LinkStorage, NamedTypes, NamedTypesDecorator, PinnedTypesAccess, PinnedTypesDecorator,
};
use tempfile::NamedTempFile;

#[test]
fn decorator_exposes_link_storage_operations_and_named_types() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let names_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let names_path = names_file.path().to_str().unwrap();

    let mut decorator = NamedTypesDecorator::with_names_database_path(db_path, names_path, false)?;

    let link = decorator.get_or_create(10, 20);
    let name_link = decorator.set_name(link, "TestLink")?;

    assert!(decorator.links().exists(link));
    assert!(decorator.names_links().exists(name_link));
    assert_eq!(Some("TestLink".to_string()), decorator.get_name(link)?);
    assert_eq!(Some(link), decorator.get_by_name("TestLink")?);

    Ok(())
}

#[test]
fn decorator_includes_pinned_types_decorator() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let names_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let names_path = names_file.path().to_str().unwrap();

    let links = LinkStorage::new(db_path, false)?;
    let names_links = LinkStorage::new(names_path, false)?;
    let pinned_types_decorator = PinnedTypesDecorator::from_link_storage(links);
    let mut decorator =
        NamedTypesDecorator::from_pinned_types_decorator(pinned_types_decorator, names_links);

    assert_eq!(vec![1, 2, 3], decorator.pinned_types(3)?);
    assert!(decorator.pinned_types_decorator().exists(1));
    assert!(decorator.pinned_types_decorator().exists(2));
    assert!(decorator.pinned_types_decorator().exists(3));

    let link = decorator.get_or_create(10, 20);
    decorator.set_name(link, "PinnedNamedLink")?;

    assert_eq!(Some(link), decorator.get_by_name("PinnedNamedLink")?);

    Ok(())
}

#[test]
fn delete_removes_associated_name_from_names_database() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let names_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let names_path = names_file.path().to_str().unwrap();

    let mut decorator = NamedTypesDecorator::with_names_database_path(db_path, names_path, false)?;
    let link = decorator.create(1, 2);
    decorator.set_name(link, "TemporaryName")?;

    assert_eq!(Some(link), decorator.get_by_name("TemporaryName")?);

    let deleted = decorator.delete(link)?;

    assert_eq!(link, deleted.index);
    assert!(!decorator.links().exists(link));
    assert_eq!(None, decorator.get_name(link)?);
    assert_eq!(None, decorator.get_by_name("TemporaryName")?);

    Ok(())
}

#[test]
fn setting_second_name_replaces_first_name() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let names_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let names_path = names_file.path().to_str().unwrap();

    let mut decorator = NamedTypesDecorator::with_names_database_path(db_path, names_path, false)?;
    let link = decorator.create(1, 1);

    decorator.set_name(link, "First")?;
    decorator.set_name(link, "Second")?;

    assert_eq!(Some("Second".to_string()), decorator.get_name(link)?);
    assert_eq!(None, decorator.get_by_name("First")?);
    assert_eq!(Some(link), decorator.get_by_name("Second")?);

    Ok(())
}

#[test]
fn reassigning_existing_name_moves_name_to_new_link() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let names_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let names_path = names_file.path().to_str().unwrap();

    let mut decorator = NamedTypesDecorator::with_names_database_path(db_path, names_path, false)?;
    let first_link = decorator.create(10, 11);
    let second_link = decorator.create(20, 21);

    decorator.set_name(first_link, "SharedName")?;
    decorator.set_name(second_link, "SharedName")?;

    assert_eq!(None, decorator.get_name(first_link)?);
    assert_eq!(
        Some("SharedName".to_string()),
        decorator.get_name(second_link)?
    );
    assert_eq!(Some(second_link), decorator.get_by_name("SharedName")?);

    Ok(())
}

#[test]
fn default_names_database_path_matches_csharp_convention() {
    assert_eq!(
        "test.names.links",
        NamedTypesDecorator::make_names_database_filename("test.db")
            .to_str()
            .unwrap()
    );
    assert_eq!(
        "a.b.names.links",
        NamedTypesDecorator::make_names_database_filename("a.b.c")
            .to_str()
            .unwrap()
    );
}

#[test]
fn decorator_can_be_built_from_existing_link_storages() -> Result<()> {
    let db_file = NamedTempFile::new()?;
    let names_file = NamedTempFile::new()?;
    let db_path = db_file.path().to_str().unwrap();
    let names_path = names_file.path().to_str().unwrap();

    let links = LinkStorage::new(db_path, false)?;
    let names_links = LinkStorage::new(names_path, false)?;
    let mut decorator = NamedTypesDecorator::from_link_storages(links, names_links);

    let link = decorator.create(5, 6);
    decorator.set_name(link, "ExistingStores")?;

    assert_eq!(Some(link), decorator.get_by_name("ExistingStores")?);
    assert_eq!(
        Some("ExistingStores".to_string()),
        decorator.get_name(link)?
    );

    Ok(())
}
