//! Tests for the QueryProcessor module

use anyhow::Result;
use link_cli::{LinkStorage, QueryProcessor};
use tempfile::NamedTempFile;

#[test]
fn test_query_processor_create() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let processor = QueryProcessor::new(false);

    // Create a simple link: (() ((1 2)))
    let changes = processor.process_query(&mut storage, "(()((1 2)))")?;

    assert!(!changes.is_empty());
    assert!(changes[0].0.is_none()); // No before (creation)
    assert!(changes[0].1.is_some()); // Has after

    Ok(())
}

#[test]
fn test_query_processor_empty() -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let processor = QueryProcessor::new(false);

    let changes = processor.process_query(&mut storage, "")?;
    assert!(changes.is_empty());

    Ok(())
}

// ============================================
// Link Deduplication Tests (Issue #65)
// ============================================

#[test]
fn test_deduplicate_duplicate_pair_with_named_links() -> Result<()> {
    // Issue #65: Test deduplication of (m a) (m a) pattern
    // Query: () (((m a) (m a)))
    // Expected: m, a (named self-refs), link for (m a), link for ((m a) (m a))
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let processor = QueryProcessor::new(false);

    processor.process_query(&mut storage, "(() (((m a) (m a))))")?;

    let all_links = storage.all();
    assert_eq!(all_links.len(), 4);

    // Get the named link IDs
    let m_id = storage.get_by_name("m").expect("m should exist");
    let a_id = storage.get_by_name("a").expect("a should exist");

    // m and a should be self-referencing
    let m_link = storage.get(m_id).unwrap();
    assert_eq!(m_link.source, m_id);
    assert_eq!(m_link.target, m_id);

    let a_link = storage.get(a_id).unwrap();
    assert_eq!(a_link.source, a_id);
    assert_eq!(a_link.target, a_id);

    // Find the (m a) link
    let ma_id = storage.search(m_id, a_id).expect("(m a) link should exist");

    // Find the outer link ((m a) (m a)) - should have same source and target
    let outer_id = storage
        .search(ma_id, ma_id)
        .expect("((m a) (m a)) link should exist");
    let outer_link = storage.get(outer_id).unwrap();
    assert_eq!(
        outer_link.source, outer_link.target,
        "Outer link should reference the same deduplicated sub-link"
    );

    Ok(())
}

#[test]
fn test_deduplicate_duplicate_pair_with_numeric_links() -> Result<()> {
    // Issue #65: Test deduplication with numeric IDs
    // Query: () (((1 2) (1 2)))
    // When using numeric IDs directly, they are treated as references (not creating self-refs)
    // So (1 2) creates link with source=1, target=2
    // The deduplication still works: ((1 2) (1 2)) creates only one (1 2) link
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let processor = QueryProcessor::new(false);

    processor.process_query(&mut storage, "(() (((1 2) (1 2))))")?;

    let all_links = storage.all();

    // Should have 2 links: (1 2) and ((1 2) (1 2))
    assert_eq!(all_links.len(), 2);

    // Link 1 should be (1 2) - the deduplicated sub-link
    let link1 = storage.get(1).expect("Link 1 should exist");
    assert_eq!(link1.source, 1);
    assert_eq!(link1.target, 2);

    // Link 2 should be (1 1) - referencing the same sub-link twice
    let link2 = storage.get(2).expect("Link 2 should exist");
    assert_eq!(link2.source, 1);
    assert_eq!(link2.target, 1);

    Ok(())
}

#[test]
fn test_deduplicate_triple_duplicate_pair() -> Result<()> {
    // Test with three identical pairs using named links: (((a b) ((a b) (a b))))
    // The (a b) should only be created once
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let processor = QueryProcessor::new(false);

    processor.process_query(&mut storage, "(() (((a b) ((a b) (a b)))))")?;

    let all_links = storage.all();
    assert_eq!(all_links.len(), 5);

    let a_id = storage.get_by_name("a").expect("a should exist");
    let b_id = storage.get_by_name("b").expect("b should exist");

    // a and b should be self-referencing
    let a_link = storage.get(a_id).unwrap();
    assert_eq!(a_link.source, a_id);
    assert_eq!(a_link.target, a_id);

    let b_link = storage.get(b_id).unwrap();
    assert_eq!(b_link.source, b_id);
    assert_eq!(b_link.target, b_id);

    // Find (a b) link - the deduplicated sub-link
    let ab_id = storage.search(a_id, b_id).expect("(a b) link should exist");

    // Find ((a b) (a b)) - should reference (a b) twice
    let inner_id = storage
        .search(ab_id, ab_id)
        .expect("((a b) (a b)) link should exist");

    // Find outer link ((a b) ((a b) (a b)))
    let outer_id = storage
        .search(ab_id, inner_id)
        .expect("outer link should exist");
    assert!(outer_id > 0);

    Ok(())
}

#[test]
fn test_deduplicate_with_different_pairs() -> Result<()> {
    // Test that different pairs are NOT deduplicated
    // Query: () (((a b) (b a))) - using named links
    // (a b) and (b a) are different and should both be created
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let processor = QueryProcessor::new(false);

    processor.process_query(&mut storage, "(() (((a b) (b a))))")?;

    let all_links = storage.all();
    assert_eq!(all_links.len(), 5);

    let a_id = storage.get_by_name("a").expect("a should exist");
    let b_id = storage.get_by_name("b").expect("b should exist");

    // a and b should be self-referencing
    let a_link = storage.get(a_id).unwrap();
    assert_eq!(a_link.source, a_id);
    assert_eq!(a_link.target, a_id);

    let b_link = storage.get(b_id).unwrap();
    assert_eq!(b_link.source, b_id);
    assert_eq!(b_link.target, b_id);

    // Find (a b) link
    let ab_id = storage.search(a_id, b_id).expect("(a b) link should exist");

    // Find (b a) link
    let ba_id = storage.search(b_id, a_id).expect("(b a) link should exist");

    // Find outer link ((a b) (b a)) - should have different source and target
    let outer_id = storage
        .search(ab_id, ba_id)
        .expect("outer link should exist");
    let outer_link = storage.get(outer_id).unwrap();
    assert_ne!(outer_link.source, outer_link.target);

    Ok(())
}

#[test]
fn test_deduplicate_nested_duplicates() -> Result<()> {
    // Test deeply nested deduplication using named links
    // Query: () ((((x y) (x y)) ((x y) (x y))))
    // (x y) is duplicated at multiple levels
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let processor = QueryProcessor::new(false);

    processor.process_query(&mut storage, "(() ((((x y) (x y)) ((x y) (x y)))))")?;

    let all_links = storage.all();
    assert_eq!(all_links.len(), 5);

    let x_id = storage.get_by_name("x").expect("x should exist");
    let y_id = storage.get_by_name("y").expect("y should exist");

    // x and y should be self-referencing
    let x_link = storage.get(x_id).unwrap();
    assert_eq!(x_link.source, x_id);
    assert_eq!(x_link.target, x_id);

    let y_link = storage.get(y_id).unwrap();
    assert_eq!(y_link.source, y_id);
    assert_eq!(y_link.target, y_id);

    // Find (x y) - the base link
    let xy_id = storage.search(x_id, y_id).expect("(x y) link should exist");

    // Find ((x y) (x y)) - references (x y) twice (deduplicated)
    let level1_id = storage
        .search(xy_id, xy_id)
        .expect("((x y) (x y)) link should exist");

    // Find (((x y) (x y)) ((x y) (x y))) - references level1 twice (deduplicated)
    let level2_id = storage
        .search(level1_id, level1_id)
        .expect("outer link should exist");
    assert!(level2_id > 0);

    Ok(())
}

#[test]
fn test_deduplicate_named_links_multiple_queries() -> Result<()> {
    // Issue #65: Verify that named links maintain consistent IDs across operations
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let processor = QueryProcessor::new(false);

    // First create named links
    processor.process_query(&mut storage, "(() ((p: p p)))")?;
    processor.process_query(&mut storage, "(() ((a: a a)))")?;

    let p_id = storage.get_by_name("p").expect("p should exist");
    let a_id = storage.get_by_name("a").expect("a should exist");

    // Now create ((p a) (p a)) - should reuse existing p and a
    processor.process_query(&mut storage, "(() (((p a) (p a))))")?;

    // p and a should still have the same IDs
    assert_eq!(storage.get_by_name("p"), Some(p_id));
    assert_eq!(storage.get_by_name("a"), Some(a_id));

    // Verify p and a are still self-referencing
    let p_link = storage.get(p_id).unwrap();
    assert_eq!(p_link.source, p_id);
    assert_eq!(p_link.target, p_id);

    let a_link = storage.get(a_id).unwrap();
    assert_eq!(a_link.source, a_id);
    assert_eq!(a_link.target, a_id);

    // Find (p a) link
    let pa_id = storage.search(p_id, a_id).expect("(p a) link should exist");

    // Find ((p a) (p a)) link - should reference pa_id twice
    let outer_id = storage
        .search(pa_id, pa_id)
        .expect("((p a) (p a)) link should exist");
    let outer_link = storage.get(outer_id).unwrap();
    assert_eq!(outer_link.source, pa_id);
    assert_eq!(outer_link.target, pa_id);

    Ok(())
}

#[test]
fn test_deduplicate_mixed_named_and_numeric() -> Result<()> {
    // Test that named links are reused across queries
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();

    let mut storage = LinkStorage::new(db_path, false)?;
    let processor = QueryProcessor::new(false);

    // First query creates (m a)
    processor.process_query(&mut storage, "(() ((m a)))")?;

    let m_id = storage.get_by_name("m").expect("m should exist");
    let a_id = storage.get_by_name("a").expect("a should exist");

    // Second query should reuse existing m and a links
    processor.process_query(&mut storage, "(() (((m a) (m a))))")?;

    // m and a should still have the same IDs
    assert_eq!(storage.get_by_name("m"), Some(m_id));
    assert_eq!(storage.get_by_name("a"), Some(a_id));

    // Should have 4 links total: m, a, (m a), ((m a) (m a))
    let all_links = storage.all();
    assert_eq!(all_links.len(), 4);

    Ok(())
}
