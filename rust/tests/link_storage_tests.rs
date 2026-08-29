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

#[test]
fn test_format_structure_renders_left_branch_with_link_indexes() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let first = storage.create(0, 0);
    let second = storage.create(first, first);
    let third = storage.create(second, first);
    let fourth = storage.create(third, second);

    assert_eq!(
        storage.format_structure(fourth)?,
        "(4: (3: (2: (1: 0 0) 1) 1) 2)"
    );

    Ok(())
}

#[test]
fn test_format_structure_renders_repeated_source_and_target_as_reference_on_right() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let first = storage.create(0, 0);
    let second = storage.create(first, first);
    storage.create(second, first);
    let fourth = storage.create(second, second);

    assert_eq!(storage.format_structure(fourth)?, "(4: (2: (1: 0 0) 1) 2)");

    Ok(())
}

/// `LinkStorage::get_or_create` must match `(source, target)` literally.
///
/// `LinkStorage` also implements the upstream `doublets` traits — including
/// for `&mut LinkStorage`, so a borrowed store can be decorated — and inside an
/// inherent `&mut self` method the receiver's type is exactly
/// `&mut LinkStorage`. Method resolution reaches the trait impl on the
/// reference before it derefs to the inherent impl, so a bare `self.search(..)`
/// resolves to `Doublets::search`, which treats `LinksConstants::any` as a
/// wildcard. That made `get_or_create(any, target)` hand back an unrelated
/// existing link instead of creating a new one.
#[test]
fn get_or_create_matches_service_constants_literally() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let mut storage = LinkStorage::new(temp_file.path().to_str().unwrap(), false)?;

    let existing = storage.create(0, 0);
    storage.update(existing, 7, 42)?;

    // Every `doublets` service constant, plus the external references that
    // encode to them, must be stored as an ordinary value.
    for reserved in [
        u32::MAX,
        u32::MAX - 1,
        u32::MAX - 2,
        u32::MAX - 3,
        u32::MAX - 4,
    ] {
        let created = storage.get_or_create(reserved, 42);
        assert_ne!(
            existing, created,
            "source {reserved} must not be treated as a wildcard"
        );
        assert_eq!(
            Some(&link_cli::Link::new(created, reserved, 42)),
            storage.get(created)
        );
        // A second call has to find the link it just created.
        assert_eq!(created, storage.get_or_create(reserved, 42));
    }

    Ok(())
}

/// The address a new link gets is part of what a query reports, so the store
/// hands out addresses the way `ResizableDirectMemoryLinks` does: a freed one
/// before a fresh one.
#[test]
fn test_freed_address_is_reused_before_the_store_grows() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let mut storage = LinkStorage::new(temp_file.path(), false)?;

    let first = storage.create(1, 1);
    let second = storage.create(2, 2);
    let third = storage.create(3, 3);
    assert_eq!((first, second, third), (1, 2, 3));

    storage.delete_raw(second)?;

    assert_eq!(storage.create(1, 3), second);
    assert_eq!(storage.create(3, 1), 4);

    Ok(())
}

/// Freed addresses come back in the reverse of the order they were freed: the
/// C# store pushes each one onto the head of its free list and allocates from
/// that same head.
#[test]
fn test_freed_addresses_are_reused_most_recently_freed_first() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let mut storage = LinkStorage::new(temp_file.path(), false)?;

    for part in 1..=5 {
        storage.create(part, part);
    }
    storage.delete_raw(2)?;
    storage.delete_raw(4)?;

    assert_eq!(storage.create(1, 3), 4);
    assert_eq!(storage.create(3, 1), 2);
    assert_eq!(storage.create(1, 5), 6);

    Ok(())
}

/// Freeing the highest address shrinks the store instead of leaving a hole,
/// and takes the freed addresses that have become the end with it.
#[test]
fn test_freeing_the_last_link_shrinks_the_store() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let mut storage = LinkStorage::new(temp_file.path(), false)?;

    for part in 1..=3 {
        storage.create(part, part);
    }
    storage.delete_raw(2)?;
    storage.delete_raw(3)?;

    // Both 2 and 3 are gone, but as a shrink rather than as two holes, so the
    // next links get them in ascending order.
    assert_eq!(storage.create(1, 2), 2);
    assert_eq!(storage.create(2, 1), 3);

    Ok(())
}

/// The free list decides the next address and nothing in the stored links
/// implies it, so it has to survive a save and a load.
#[test]
fn test_free_list_survives_a_reload() -> Result<()> {
    let temp_file = NamedTempFile::new()?;

    {
        let mut storage = LinkStorage::new(temp_file.path(), false)?;
        for part in 1..=5 {
            storage.create(part, part);
        }
        storage.delete_raw(2)?;
        storage.delete_raw(4)?;
        storage.save()?;
    }

    let mut storage = LinkStorage::new(temp_file.path(), false)?;
    assert_eq!(storage.create(1, 3), 4);
    assert_eq!(storage.create(3, 1), 2);
    assert_eq!(storage.create(1, 5), 6);

    Ok(())
}

/// A database written before the free list was recorded — or by hand — still
/// loads, with the addresses missing below the highest stored link recovered
/// as freed ones, lowest reused first.
#[test]
fn test_missing_addresses_are_recovered_from_a_database_without_a_free_list() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    std::fs::write(temp_file.path(), "(1 1 1)\n(3 3 3)\n(5 5 5)\n")?;

    let mut storage = LinkStorage::new(temp_file.path(), false)?;
    assert_eq!(storage.create(1, 3), 2);
    assert_eq!(storage.create(3, 1), 4);
    assert_eq!(storage.create(1, 5), 6);

    Ok(())
}

/// Reaching a requested address allocates the addresses before it, and those
/// are freed again — `EnsureCreated` deletes every link it did not ask for.
#[test]
fn test_ensure_created_frees_the_addresses_it_passed_over() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let mut storage = LinkStorage::new(temp_file.path(), false)?;

    storage.create(1, 1);
    assert_eq!(storage.ensure_created(4), 4);

    assert!(!storage.exists(2));
    assert!(!storage.exists(3));
    assert_eq!(
        storage.get(4).map(|link| (link.source, link.target)),
        Some((0, 0))
    );

    // 2 and 3 were freed in the order they were created, leaving 3 on top.
    assert_eq!(storage.create(1, 4), 3);
    assert_eq!(storage.create(4, 1), 2);

    Ok(())
}
