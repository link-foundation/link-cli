use anyhow::Result;
use link_cli::{external_reference, LinkStorage, NamedLinks, UnicodeStringStorage};
use tempfile::NamedTempFile;

fn with_storage(test: impl FnOnce(&mut UnicodeStringStorage<'_>) -> Result<()>) -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();
    let mut links = LinkStorage::new(db_path, false)?;
    let mut storage = UnicodeStringStorage::new(&mut links)?;
    test(&mut storage)
}

#[test]
fn create_and_retrieve_empty_string() -> Result<()> {
    with_storage(|storage| {
        let empty = storage.create_string("")?;
        assert_eq!("", storage.get_string(empty)?);
        Ok(())
    })
}

#[test]
fn create_and_retrieve_simple_string() -> Result<()> {
    with_storage(|storage| {
        let hello = storage.create_string("Hello")?;
        assert_eq!("Hello", storage.get_string(hello)?);
        Ok(())
    })
}

#[test]
fn create_and_retrieve_multiple_strings() -> Result<()> {
    with_storage(|storage| {
        let first = storage.create_string("First")?;
        let second = storage.create_string("Second")?;

        assert_eq!("First", storage.get_string(first)?);
        assert_eq!("Second", storage.get_string(second)?);
        Ok(())
    })
}

#[test]
fn create_and_retrieve_unicode_string_as_utf16_sequence() -> Result<()> {
    with_storage(|storage| {
        let content = "Hello, 世界! Привет, мир! 😀";
        let link = storage.create_string(content)?;

        assert_eq!(content, storage.get_string(link)?);
        assert!(storage.unicode_sequence_code_units(link)?.len() > content.chars().count());
        Ok(())
    })
}

#[test]
fn pinned_types_are_created_and_named() -> Result<()> {
    with_storage(|storage| {
        assert_eq!(1, storage.type_type());
        assert_eq!(2, storage.unicode_symbol_type());
        assert_eq!(3, storage.unicode_sequence_type());
        assert_eq!(4, storage.string_type());
        assert_eq!(5, storage.empty_string_type());
        assert_eq!(6, storage.name_type());

        for (id, name) in [
            (storage.type_type(), "Type"),
            (storage.unicode_symbol_type(), "UnicodeSymbol"),
            (storage.unicode_sequence_type(), "UnicodeSequence"),
            (storage.string_type(), "String"),
            (storage.empty_string_type(), "EmptyString"),
            (storage.name_type(), "Name"),
        ] {
            assert_eq!(Some(id), storage.get_by_name(name)?);
            assert_eq!(Some(name.to_string()), storage.get_name(id)?);
        }

        Ok(())
    })
}

#[test]
fn create_and_retrieve_user_defined_type() -> Result<()> {
    with_storage(|storage| {
        let user_type = storage.get_or_create_type("UserType")?;
        assert_eq!(Some(user_type), storage.get_by_name("UserType")?);
        assert_eq!(Some("UserType".to_string()), storage.get_name(user_type)?);
        Ok(())
    })
}

#[test]
fn name_external_reference_matches_csharp_hybrid_encoding() -> Result<()> {
    with_storage(|storage| {
        assert_eq!(u32::MAX, external_reference(1));

        storage.set_name_for_external_reference(1, "MyExternalReference")?;

        assert_eq!(
            Some("MyExternalReference".to_string()),
            storage.get_name_by_external_reference(1)?
        );
        assert_eq!(
            Some(1),
            storage.get_external_reference_by_name("MyExternalReference")?
        );
        assert_eq!(
            Some("MyExternalReference".to_string()),
            storage.get_name(external_reference(1))?
        );

        Ok(())
    })
}

#[test]
fn name_is_removed_when_link_is_deleted() -> Result<()> {
    with_storage(|storage| {
        let link = storage.links_mut().create(0, 0);
        storage.set_name(link, "TestName")?;
        assert_eq!(Some("TestName".to_string()), storage.get_name(link)?);
        assert_eq!(Some(link), storage.get_by_name("TestName")?);

        storage.links_mut().delete(link)?;
        storage.remove_name(link)?;

        assert_eq!(None, storage.get_by_name("TestName")?);
        assert_eq!(None, storage.get_name(link)?);
        Ok(())
    })
}

#[test]
fn deleting_non_named_link_does_not_affect_other_names() -> Result<()> {
    with_storage(|storage| {
        let named_link = storage.links_mut().create(0, 0);
        storage.set_name(named_link, "Named")?;
        let unnamed_link = storage.links_mut().create(0, 0);

        storage.links_mut().delete(unnamed_link)?;

        assert_eq!(Some(named_link), storage.get_by_name("Named")?);
        assert_eq!(Some("Named".to_string()), storage.get_name(named_link)?);
        Ok(())
    })
}

#[test]
fn name_is_removed_when_external_reference_is_deleted() -> Result<()> {
    with_storage(|storage| {
        storage.set_name_for_external_reference(123, "ExternalName")?;
        assert_eq!(
            Some("ExternalName".to_string()),
            storage.get_name_by_external_reference(123)?
        );
        assert_eq!(
            Some(123),
            storage.get_external_reference_by_name("ExternalName")?
        );

        storage.remove_name_by_external_reference(123)?;

        assert_eq!(
            None,
            storage.get_external_reference_by_name("ExternalName")?
        );
        assert_eq!(None, storage.get_name_by_external_reference(123)?);
        Ok(())
    })
}

#[test]
fn named_links_facade_matches_csharp_named_links_role() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();
    let mut links = LinkStorage::new(db_path, false)?;
    let mut named_links = NamedLinks::new(&mut links)?;

    let link = named_links.unicode_storage_mut().links_mut().create(0, 0);
    named_links.set_name(link, "FacadeName")?;

    assert_eq!(Some(link), named_links.get_by_name("FacadeName")?);
    assert_eq!(Some("FacadeName".to_string()), named_links.get_name(link)?);
    Ok(())
}
