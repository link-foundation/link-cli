use anyhow::Result;
use link_cli::{NamedTypes, NamedTypesDecorator, PinnedTypesAccess, PinnedTypesDecorator};
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn decorator_exposes_link_storage_operations_and_named_types() -> Result<()> {
    let (_files, mut decorator) = test_store::named()?;

    let link = decorator.get_or_create(10, 20);
    let name_link = decorator.set_name(link, "TestLink")?;

    assert!(decorator.links().exists(link));
    assert!(decorator.names_links().exists(name_link));
    assert_eq!(Some("TestLink".to_string()), decorator.get_name(link)?);
    assert_eq!(Some(link), decorator.get_by_name("TestLink")?);

    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn decorator_includes_pinned_types_decorator() -> Result<()> {
    let (_db_file, links) = test_store::links()?;
    let (_names_file, names_links) = test_store::links()?;
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn delete_removes_associated_name_from_names_database() -> Result<()> {
    let (_files, mut decorator) = test_store::named()?;
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn setting_second_name_replaces_first_name() -> Result<()> {
    let (_files, mut decorator) = test_store::named()?;
    let link = decorator.create(1, 1);

    decorator.set_name(link, "First")?;
    decorator.set_name(link, "Second")?;

    assert_eq!(Some("Second".to_string()), decorator.get_name(link)?);
    assert_eq!(None, decorator.get_by_name("First")?);
    assert_eq!(Some(link), decorator.get_by_name("Second")?);

    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn reassigning_existing_name_moves_name_to_new_link() -> Result<()> {
    let (_files, mut decorator) = test_store::named()?;
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn decorator_can_be_built_from_existing_link_storages() -> Result<()> {
    let (_db_file, links) = test_store::links()?;
    let (_names_file, names_links) = test_store::links()?;
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

/// `UnicodeStringStorage` names its own pinned types — `Type`, `Name`,
/// `String`, `UnicodeSymbol`, `UnicodeSequence` and `EmptyString` — inside the
/// names database, so a user link that takes one of those names shares its name
/// link with a pinned type.
///
/// Resolving the name then has to pick the user link out of several holders.
/// Before this was handled, `get_by_name` returned whichever holder the names
/// hash map happened to yield first, and the lookup failed (or succeeded)
/// depending on hash order.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn reserved_pinned_type_names_can_still_be_used_for_user_links() -> Result<()> {
    let (_files, mut decorator) = test_store::named()?;

    let reserved = [
        "Type",
        "Name",
        "String",
        "UnicodeSymbol",
        "UnicodeSequence",
        "EmptyString",
    ];
    let mut created = Vec::new();
    for name in reserved {
        let link = decorator.create(0, 0);
        decorator.set_name(link, name)?;
        created.push((name, link));
    }

    // Every name still resolves after all the others were added.
    for (name, link) in &created {
        assert_eq!(
            Some(*link),
            decorator.get_by_name(name)?,
            "the user link named {name} must win over the pinned type of the same name"
        );
        assert_eq!(Some((*name).to_string()), decorator.get_name(*link)?);
    }

    Ok(())
}
