//! C# AdvancedMixedQueryProcessor parity tests.

use anyhow::Result;
use link_cli::{Link, LinkStorage, QueryProcessor};
use tempfile::NamedTempFile;

fn with_storage(test: impl FnOnce(&mut LinkStorage, &QueryProcessor) -> Result<()>) -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();
    let mut storage = LinkStorage::new(db_path, false)?;
    let processor = QueryProcessor::new(false);
    test(&mut storage, &processor)
}

fn sorted_links(storage: &LinkStorage) -> Vec<Link> {
    let mut links: Vec<Link> = storage.all().into_iter().copied().collect();
    links.sort_by_key(|link| link.index);
    links
}

fn assert_link_exists(storage: &LinkStorage, index: u32, source: u32, target: u32) {
    let link = storage
        .get(index)
        .unwrap_or_else(|| panic!("missing link {index}: {source} {target}"));
    assert_eq!(*link, Link::new(index, source, target));
}

#[test]
fn test_unwrapped_create_query_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "() ((1: 1 1))")?;

        assert_eq!(storage.all().len(), 1);
        assert_link_exists(storage, 1, 1, 1);
        Ok(())
    })
}

#[test]
fn test_create_explicit_index_after_gap_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((3: 3 3)))")?;

        assert_eq!(storage.all().len(), 1);
        assert_link_exists(storage, 3, 3, 3);
        Ok(())
    })
}

#[test]
fn test_create_deep_nested_numeric_links_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() (((1 1) ((2 2) ((3 3) ((4 4) (5 5)))))))")?;

        assert_eq!(storage.all().len(), 9);
        assert_link_exists(storage, 1, 1, 1);
        assert_link_exists(storage, 2, 2, 2);
        assert_link_exists(storage, 3, 3, 3);
        assert_link_exists(storage, 4, 4, 4);
        assert_link_exists(storage, 5, 5, 5);
        assert_link_exists(storage, 6, 4, 5);
        assert_link_exists(storage, 7, 3, 6);
        assert_link_exists(storage, 8, 2, 7);
        assert_link_exists(storage, 9, 1, 8);
        Ok(())
    })
}

#[test]
fn test_delete_by_source_target_pattern_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((1 2)))")?;
        processor.process_query(storage, "(() ((2 2)))")?;

        processor.process_query(storage, "(((1 2)) ())")?;

        assert_eq!(storage.all().len(), 1);
        assert_link_exists(storage, 2, 2, 2);
        Ok(())
    })
}

#[test]
fn test_delete_by_wildcard_target_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((1 2) (2 2)))")?;

        processor.process_query(storage, "(((1 *)) ())")?;

        assert_eq!(storage.all().len(), 1);
        assert_link_exists(storage, 2, 2, 2);
        Ok(())
    })
}

#[test]
fn test_delete_all_by_index_wildcard_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((1 2) (2 2)))")?;

        processor.process_query(storage, "(((*:)) ())")?;

        assert!(storage.all().is_empty());
        Ok(())
    })
}

#[test]
fn test_swap_all_links_using_variables_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((1 2) (2 1)))")?;

        processor.process_query(
            storage,
            "((($index: $source $target)) (($index: $target $source)))",
        )?;

        assert_eq!(storage.all().len(), 2);
        assert_link_exists(storage, 1, 2, 1);
        assert_link_exists(storage, 2, 1, 2);
        Ok(())
    })
}

#[test]
fn test_no_op_variable_query_returns_matched_changes() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((1 1)))")?;
        processor.process_query(storage, "(() ((2 2)))")?;

        let changes = processor.process_query(
            storage,
            "((($index: $source $target)) (($index: $source $target)))",
        )?;

        assert_eq!(
            sorted_links(storage),
            vec![Link::new(1, 1, 1), Link::new(2, 2, 2)]
        );
        assert_eq!(changes.len(), 2);
        assert!(changes.contains(&(Some(Link::new(1, 1, 1)), Some(Link::new(1, 1, 1)))));
        assert!(changes.contains(&(Some(Link::new(2, 2, 2)), Some(Link::new(2, 2, 2)))));
        Ok(())
    })
}

#[test]
fn test_named_link_rename_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((child: father mother)))")?;

        processor.process_query(storage, "(((child: father mother)) ((son: father mother)))")?;

        assert_eq!(storage.get_by_name("child"), None);
        let son_id = storage.get_by_name("son").expect("son should exist");
        let father_id = storage.get_by_name("father").expect("father should exist");
        let mother_id = storage.get_by_name("mother").expect("mother should exist");
        assert_link_exists(storage, son_id, father_id, mother_id);
        assert_eq!(storage.all().len(), 3);
        Ok(())
    })
}

#[test]
fn test_delete_by_names_keeps_leaf_names_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((child: father mother)))")?;

        processor.process_query(storage, "(((child: father mother)) ())")?;

        assert_eq!(storage.get_by_name("child"), None);
        assert!(storage.get_by_name("father").is_some());
        assert!(storage.get_by_name("mother").is_some());
        assert_eq!(storage.all().len(), 2);
        Ok(())
    })
}

#[test]
fn test_unknown_named_restriction_matches_nothing() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((known: left right)))")?;

        let changes = processor.process_query(storage, "(((unknown: left right)) ())")?;

        assert!(changes.is_empty());
        assert_eq!(storage.all().len(), 3);
        assert!(storage.get_by_name("known").is_some());
        assert!(storage.get_by_name("unknown").is_none());
        Ok(())
    })
}

#[test]
fn test_string_composite_left_child_does_not_create_extra_leaf() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((type: type type)))")?;
        processor.process_query(storage, "(() ((link: link type)))")?;

        let type_id = storage.get_by_name("type").expect("type should exist");
        let link_id = storage.get_by_name("link").expect("link should exist");
        assert_eq!(storage.all().len(), 2);
        assert_link_exists(storage, type_id, type_id, type_id);
        assert_link_exists(storage, link_id, link_id, type_id);
        Ok(())
    })
}
