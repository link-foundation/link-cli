//! C# AdvancedMixedQueryProcessor parity tests.

use anyhow::Result;
use link_cli::{Link, NamedTypes, NamedTypesDecorator, QueryProcessor};
use tempfile::NamedTempFile;

fn with_storage(
    test: impl FnOnce(&mut NamedTypesDecorator, &QueryProcessor) -> Result<()>,
) -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let names_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();
    let names_path = names_file.path().to_str().unwrap();
    let mut storage = NamedTypesDecorator::with_names_database_path(db_path, names_path, false)?;
    let processor = QueryProcessor::new(false).with_auto_create_missing_references(true);
    test(&mut storage, &processor)
}

fn sorted_links(storage: &NamedTypesDecorator) -> Vec<Link> {
    let mut links: Vec<Link> = storage.all().into_iter().copied().collect();
    links.sort_by_key(|link| link.index);
    links
}

fn assert_link_exists(storage: &NamedTypesDecorator, index: u32, source: u32, target: u32) {
    let link = storage
        .get(index)
        .unwrap_or_else(|| panic!("missing link {index}: {source} {target}"));
    assert_eq!(*link, Link::new(index, source, target));
}

fn name_id(storage: &mut NamedTypesDecorator, name: &str) -> Result<u32> {
    Ok(storage
        .get_by_name(name)?
        .unwrap_or_else(|| panic!("{name} should exist")))
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
        processor.process_query(storage, "(() ((1: 1 1) (2: 2 2) (3: 1 2)))")?;

        processor.process_query(storage, "(((1 2)) ())")?;

        assert_eq!(storage.all().len(), 2);
        assert_link_exists(storage, 1, 1, 1);
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

        assert_eq!(storage.get_by_name("child")?, None);
        let son_id = name_id(storage, "son")?;
        let father_id = name_id(storage, "father")?;
        let mother_id = name_id(storage, "mother")?;
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

        assert_eq!(storage.get_by_name("child")?, None);
        assert!(storage.get_by_name("father")?.is_some());
        assert!(storage.get_by_name("mother")?.is_some());
        assert_eq!(storage.all().len(), 2);
        Ok(())
    })
}

#[test]
fn test_unknown_named_restriction_fails_without_auto_create() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((known: left right)))")?;

        let strict_processor = QueryProcessor::new(false);
        let error = strict_processor
            .process_query(storage, "(((unknown: left right)) ())")
            .expect_err("unknown named restriction should fail validation");

        assert!(error.to_string().contains("unknown"));
        assert!(error
            .to_string()
            .contains("--auto-create-missing-references"));
        assert!(storage.get_by_name("known")?.is_some());
        assert!(storage.get_by_name("unknown")?.is_none());
        Ok(())
    })
}

#[test]
fn test_string_composite_left_child_does_not_create_extra_leaf() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((type: type type)))")?;
        processor.process_query(storage, "(() ((link: link type)))")?;

        let type_id = name_id(storage, "type")?;
        let link_id = name_id(storage, "link")?;
        assert_eq!(storage.all().len(), 2);
        assert_link_exists(storage, type_id, type_id, type_id);
        assert_link_exists(storage, link_id, link_id, type_id);
        Ok(())
    })
}

#[test]
fn test_string_aliases_in_variable_restriction_constrain_matches_to_named_links_matches_csharp(
) -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((father: father father)))")?;
        processor.process_query(storage, "(() ((mother: mother mother)))")?;
        processor.process_query(storage, "(() ((child: father mother)))")?;

        let father_id = name_id(storage, "father")?;
        let mother_id = name_id(storage, "mother")?;
        let child_id = name_id(storage, "child")?;

        processor.process_query(storage, "((($id: father mother)) (($id: mother father)))")?;

        assert_eq!(storage.all().len(), 3);
        assert_link_exists(storage, father_id, father_id, father_id);
        assert_link_exists(storage, mother_id, mother_id, mother_id);
        assert_link_exists(storage, child_id, mother_id, father_id);
        Ok(())
    })
}

#[test]
fn test_issue_20_substitute_matched_link_and_outgoing_link_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(
            storage,
            "(() ((1: 1 1) (18: 1 21) (19: 1 20) (20: 20 20) (21: 21 21)))",
        )?;

        processor.process_query(storage, "((($i: 1 21)) (($i: $s $t) ($i 20)))")?;

        let links = sorted_links(storage);
        assert_eq!(links.len(), 6);
        assert_link_exists(storage, 1, 1, 1);
        assert_link_exists(storage, 18, 1, 21);
        assert_link_exists(storage, 19, 1, 20);
        assert_link_exists(storage, 20, 20, 20);
        assert_link_exists(storage, 21, 21, 21);

        let outgoing_links = links
            .iter()
            .filter(|link| link.source == 18 && link.target == 20)
            .collect::<Vec<_>>();
        assert_eq!(outgoing_links.len(), 1);
        assert_ne!(outgoing_links[0].index, 0);
        assert_ne!(outgoing_links[0].index, u32::MAX);
        assert!(links.iter().all(|link| {
            link.index != u32::MAX && link.source != u32::MAX && link.target != u32::MAX
        }));
        Ok(())
    })
}

#[test]
fn test_issue_20_substitute_full_point_with_unbound_parts_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((21: 21 21)))")?;

        processor.process_query(storage, "(((21: 21 21)) ((21: $s $t)))")?;

        assert_eq!(storage.all().len(), 1);
        assert_link_exists(storage, 21, 21, 21);
        assert!(storage
            .all()
            .iter()
            .all(|link| link.source != u32::MAX && link.target != u32::MAX));
        Ok(())
    })
}

/// Deleting a link deletes the links that reference it, which is what the
/// upstream cascade resolvers do and what the `cascade delete of usage`
/// scenario in `docs/case-studies/issue-100/evidence/cli-parity/run.sh`
/// compares against C#.
#[test]
fn test_delete_cascades_to_usages_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((1 1)))")?;
        processor.process_query(storage, "(() ((2 2)))")?;
        processor.process_query(storage, "(() ((1 2)))")?;

        processor.process_query(storage, "(((2: 2 2)) ())")?;

        assert_eq!(sorted_links(storage), vec![Link::new(1, 1, 1)]);
        Ok(())
    })
}

/// The cascade is transitive and leaves untouched links alone.
#[test]
fn test_delete_cascade_chain_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((1 1)))")?;
        processor.process_query(storage, "(() ((2 2)))")?;
        processor.process_query(storage, "(() ((1 2)))")?;
        processor.process_query(storage, "(() ((3 3)))")?;

        processor.process_query(storage, "(((1: 1 1)) ())")?;

        assert_eq!(sorted_links(storage), vec![Link::new(2, 2, 2)]);
        Ok(())
    })
}

/// An update that would duplicate an existing link merges into it instead of
/// creating a second copy, so the updated address disappears.
#[test]
fn test_update_into_existing_pair_merges_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((1 1)))")?;
        processor.process_query(storage, "(() ((2 2)))")?;
        processor.process_query(storage, "(() ((1 2)))")?;
        processor.process_query(storage, "(() ((2 1)))")?;

        processor.process_query(storage, "(((4: 2 1)) ((4: 1 2)))")?;

        assert_eq!(
            sorted_links(storage),
            vec![Link::new(1, 1, 1), Link::new(2, 2, 2), Link::new(3, 1, 2)]
        );
        Ok(())
    })
}

/// Deleting a named link cascades through the links that use it, and the names
/// of the survivors are untouched.
#[test]
fn test_named_delete_cascades_to_usages_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((a: a a)))")?;
        processor.process_query(storage, "(() ((b: b b)))")?;
        processor.process_query(storage, "(() ((a b)))")?;

        processor.process_query(storage, "(((a: a a)) ())")?;

        let b = name_id(storage, "b")?;
        assert_eq!(sorted_links(storage), vec![Link::new(b, b, b)]);
        Ok(())
    })
}

/// A single-link swap that does not collide with an existing pair keeps its
/// address, which is the plain-update path through the uniqueness resolver.
#[test]
fn test_swap_one_link_keeps_its_address_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "(() ((1 1) (1 2)))")?;

        processor.process_query(storage, "(((2: 1 2)) ((2: 2 1)))")?;

        assert_eq!(
            sorted_links(storage),
            vec![Link::new(1, 1, 1), Link::new(2, 2, 1)]
        );
        Ok(())
    })
}

/// A substitution variable that no restriction ever bound is *unspecified*, not
/// a literal address: C# marks it with `links.Constants.Any` and the store turns
/// that into null on the way in, so `() (($a $a))` creates the point `(1: 0 0)`.
#[test]
fn test_unbound_substitution_variable_creates_null_point_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "() (($a $a))")?;

        assert_eq!(sorted_links(storage), vec![Link::new(1, 0, 0)]);
        Ok(())
    })
}

/// The same query twice finds the link it created the first time rather than
/// creating a second one, because C#'s `SearchOrDefault` reads `any` in a lookup
/// as a wildcard.
#[test]
fn test_unbound_substitution_variable_twice_is_idempotent_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "() (($a $a))")?;
        processor.process_query(storage, "() (($a $a))")?;

        assert_eq!(sorted_links(storage), vec![Link::new(1, 0, 0)]);
        Ok(())
    })
}

/// An explicit index pins the address; the unspecified halves still land as null.
#[test]
fn test_unbound_substitution_variable_at_an_index_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "() ((5: $a $a))")?;

        assert_eq!(sorted_links(storage), vec![Link::new(5, 0, 0)]);
        Ok(())
    })
}

/// One specified half plus one unspecified half is a wildcard lookup, so it
/// finds the existing `(1: 1 1)` instead of creating `(2: 1 null)` beside it.
#[test]
fn test_unbound_substitution_variable_one_half_finds_existing_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "() ((1 1))")?;
        processor.process_query(storage, "() ((1 $a))")?;

        assert_eq!(sorted_links(storage), vec![Link::new(1, 1, 1)]);
        Ok(())
    })
}

/// `*` in a substitution is unspecified in exactly the same way a never-bound
/// variable is.
#[test]
fn test_star_in_a_substitution_creates_null_point_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "() ((* *))")?;

        assert_eq!(sorted_links(storage), vec![Link::new(1, 0, 0)]);
        Ok(())
    })
}

/// An unspecified half of an *update* keeps the half already stored, so
/// rewriting `(1: 1 1)` through `(1: $x $y)` leaves it exactly as it was.
#[test]
fn test_unbound_substitution_variable_in_an_update_keeps_existing_matches_csharp() -> Result<()> {
    with_storage(|storage, processor| {
        processor.process_query(storage, "() ((1 1))")?;
        processor.process_query(storage, "((1: 1 1)) ((1: $x $y))")?;

        assert_eq!(sorted_links(storage), vec![Link::new(1, 1, 1)]);
        Ok(())
    })
}
