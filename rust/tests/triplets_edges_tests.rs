//! The edges of the triplet encoding: doublets it does not know, a store lent
//! to a layer above and writes it has to refuse.

use anyhow::Result;
use link_cli::triplets::match_triplets_query;
use link_cli::{
    process_triplets_query, DoubletRole, LinkStorage, NamedTypeLinks, Triplet, TripletQuery,
    Triplets, TripletsOverDoublets,
};

type Store = TripletsOverDoublets<LinkStorage>;

fn store() -> Result<Store> {
    TripletsOverDoublets::with_named_markers(LinkStorage::in_memory(false).with_indexed_lookups())
}

fn asks_through_a_lent_store(mut triplets: impl Triplets) -> Result<()> {
    let markers = triplets.triplet_markers();
    let point = triplets.create_triplet_point()?;
    triplets.create_triplet_point_at(50)?;
    let triplet = triplets.create_triplet(point, 50, point)?;
    assert!(triplets.is_triplet(triplet));
    let got = triplets.get_triplet(triplet).unwrap();
    assert_eq!(got, Triplet::new(triplet, point, 50, point));
    assert_eq!(triplets.role_of(markers.pair), Some(DoubletRole::Marker));
    assert_eq!(triplets.each_triplet(TripletQuery::by_from(50)), [got]);
    assert_eq!(triplets.update_triplet(triplet, point, 0, 0)?, got);
    assert_eq!(triplets.delete_triplet(triplet)?.len(), 1);
    Ok(())
}

#[test]
fn a_lent_store_stays_a_store_of_triplets() -> Result<()> {
    let mut triplets = store()?;
    asks_through_a_lent_store(&mut triplets)?;
    assert!(triplets.is_triplet(50));
    Ok(())
}

#[test]
fn the_doublets_stay_reachable_under_the_triplets() -> Result<()> {
    let mut links = store()?;
    let first = links.create(0, 0);
    assert_eq!(links.ensure_created(first + 5), first + 5);
    assert!(links.reference_exists(first));
    assert_eq!(links.claims(first), None);
    assert_eq!(links.next_created_addresses(1)?.len(), 1);
    let pair = links.get_or_create(first, first);
    assert_eq!(links.get_or_create(first, first), pair);
    assert_eq!(links.search(first, first), Some(pair));
    assert_eq!(links.set_name(first, "named")?, first);
    assert_eq!(links.get_name(first)?.as_deref(), Some("named"));
    assert_eq!(links.get_by_name("named")?, Some(first));
    links.remove_name(first)?;
    assert_eq!(links.get_by_name("named")?, None);
    let made = links.get_or_create_named("made")?;
    assert_eq!(links.get_or_create_named("made")?, made);
    links.save()?;
    Ok(())
}

#[test]
fn a_doublet_the_encoding_does_not_know_is_foreign() -> Result<()> {
    let mut links = store()?;
    let markers = links.triplet_markers();
    let kind = links.create_triplet_point()?;
    let from = links.create_triplet_point()?;
    let to = links.create_triplet_point()?;
    let triplet = links.create_triplet(kind, from, to)?;
    let duplicate = links.create_triplet(kind, from, to)?;

    let outer = links.get_link(triplet).unwrap();
    let pair = outer.target;
    let wrapper = links.get_link(pair).unwrap().source;
    assert_eq!(links.role_of(pair), Some(DoubletRole::Pair));
    assert_eq!(links.role_of(wrapper), Some(DoubletRole::Wrapper));
    let duplicate_pair = links.get_link(duplicate).unwrap().target;
    assert_eq!(
        links.role_of(duplicate_pair),
        Some(DoubletRole::DuplicatePair)
    );

    let of_nothing = links.create(0, kind);
    let of_empty = links.create(markers.nothing, kind);
    let of_missing = links.create(9_999, kind);
    let to_missing = links.create(kind, 9_998);
    let to_marker = links.create(kind, markers.pair);
    let plain = links.create(kind, from);
    let on_duplicate = links.create(duplicate_pair, kind);
    let source_of_missing = links.create(9_997, kind);
    let on_missing = links.create(source_of_missing, kind);
    for id in [
        of_nothing,
        of_empty,
        of_missing,
        to_missing,
        to_marker,
        plain,
        on_duplicate,
        on_missing,
    ] {
        assert_eq!(
            links.role_of(id),
            Some(DoubletRole::Foreign),
            "doublet {id}"
        );
        assert_eq!(links.get_triplet(id), None, "doublet {id}");
    }
    assert_eq!(links.role_of(9_000), None);
    assert_eq!(links.get_triplet(markers.nothing), None);
    Ok(())
}

#[test]
fn a_query_by_type_or_parts_leaves_out_what_is_not_a_triplet() -> Result<()> {
    let mut links = store()?;
    let markers = links.triplet_markers();
    let kind = links.create_triplet_point()?;
    let from = links.create_triplet_point()?;
    let first = links.create_triplet(kind, from, from)?;
    let second = links.create_triplet(kind, from, from)?;
    let pair = links.get_link(first).unwrap().target;

    // A type made of a marker holds no triplets, and neither does nothing.
    assert!(links
        .each_triplet(TripletQuery::new(None, Some(markers.pair), None, None))
        .is_empty());
    assert!(links
        .each_triplet(TripletQuery::new(None, Some(0), None, None))
        .is_empty());
    // Doublets of somebody else on the pair and on the type are no triplets.
    links.create(0, pair);
    links.create(pair, 9_996);
    links.create(kind, 9_995);
    let found = links.each_triplet(TripletQuery::new(None, None, Some(from), Some(from)));
    let indexes: Vec<u32> = found.iter().map(|triplet| triplet.index).collect();
    assert_eq!(indexes, [first, second]);
    let of_kind = links.each_triplet(TripletQuery::new(None, Some(kind), None, None));
    assert!(of_kind
        .iter()
        .all(|triplet| [first, second, kind].contains(&triplet.index)));
    Ok(())
}

#[test]
fn a_write_that_would_merge_a_triplet_away_is_refused() -> Result<()> {
    let mut links = store()?;
    let kind = links.create_triplet_point()?;
    let from = links.create_triplet_point()?;
    let triplet = links.create_triplet(kind, from, from)?;
    let before = links.get_link(triplet);
    // Somebody else's doublet already is what the triplet would become.
    let taken = links.create(kind, triplet);
    let refused = links
        .update_triplet(triplet, kind, 0, 0)
        .unwrap_err()
        .to_string();
    assert!(refused.contains(&format!("already {taken}")), "{refused}");
    assert_eq!(links.get_link(triplet), before);
    assert_eq!(
        links
            .update_triplet(taken, kind, 0, 0)
            .unwrap_err()
            .to_string(),
        format!("Link {taken} is not a triplet.")
    );
    assert_eq!(
        links.delete_triplet(taken).unwrap_err().to_string(),
        format!("Link {taken} is not a triplet.")
    );
    Ok(())
}

#[test]
fn an_update_to_a_taken_pair_moves_to_a_duplicate_and_back() -> Result<()> {
    let mut links = store()?;
    let kind = links.create_triplet_point()?;
    let from = links.create_triplet_point()?;
    let to = links.create_triplet_point()?;
    let first = links.create_triplet(kind, from, to)?;
    let second = links.create_triplet(kind, to, from)?;
    links.update_triplet(second, kind, from, to)?;
    assert_eq!(
        links.get_triplet(second),
        Some(Triplet::new(second, kind, from, to))
    );
    // Away from the shared pair and back to it.
    links.update_triplet(first, kind, from, 0)?;
    links.update_triplet(first, kind, from, to)?;
    assert_eq!(
        links.get_triplet(first),
        Some(Triplet::new(first, kind, from, to))
    );
    Ok(())
}

fn seeded() -> Result<Store> {
    let mut triplets = store()?;
    process_triplets_query(&mut triplets, "() ((5: 5 0 0) (6: 5 5 5) (named: 5 0 0))")?;
    Ok(triplets)
}

#[test]
fn queries_of_a_wrong_shape_are_refused_by_name() -> Result<()> {
    const SHAPE: &str = "A triplets query is ((restriction) (substitution)).";
    for (query, expected) in [
        ("((1: 2 3 4))", SHAPE),
        ("(x: (1 2 3) (1 2 3))", SHAPE),
        ("(30)", SHAPE),
        ("(1 2 3)\n(1 2 3)\n(1 2 3)", SHAPE),
        ("((1 2)) ()", "(1 2) has 2"),
        ("((1 2 3 4 5)) ()", "(1 2 3 4 5) has 5"),
        (
            "((1 2 (3 4))) ()",
            "A part of a triplet pattern is a reference, not (3 4).",
        ),
        ("() ((99))", "(99) has 1"),
        ("() ((0: 5 0 0))", "A triplet cannot be at address 0."),
        (
            "() ((77: unknown 0 0))",
            "The type part unknown of a substitution is not a triplet.",
        ),
        (
            "() ((77: 5 unknown 0))",
            "The from part unknown of a substitution is not a triplet.",
        ),
        (
            "((($i: 5 0 0)) (($i: 1 0 0)))",
            "The type part 1 of a substitution is not a triplet.",
        ),
    ] {
        let error = process_triplets_query(&mut seeded()?, query)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{query}: {error}");
    }
    for restriction in ["(1 2 3)\n(1 2 3)", "((1 2))", "((1: (2) 3))"] {
        assert!(
            match_triplets_query(&mut seeded()?, restriction, "$i").is_err(),
            "{restriction}"
        );
    }
    Ok(())
}

#[test]
fn a_name_or_variable_written_twice_is_one_triplet() -> Result<()> {
    for query in [
        "() (($v: 5 0 0) ($v: 5 0 0))",
        "() ((n: 5 0 0) (n: 5 0 0))",
        "((6: 5 5 5)) ((6: 5 5 0) (6: 5 0 0))",
    ] {
        let changes = process_triplets_query(&mut seeded()?, query)?;
        assert_eq!(changes.len(), 1, "{query}: {changes:?}");
    }
    // The second pattern wrote the triplet last, and that is the one reported.
    let mut triplets = seeded()?;
    let changes = process_triplets_query(&mut triplets, "((6: 5 5 5)) ((6: 5 5 0) (6: 5 0 0))")?;
    assert_eq!(changes[0].after, Some(Triplet::new(6, 5, 0, 0)));
    assert_eq!(triplets.get_triplet(6), Some(Triplet::new(6, 5, 0, 0)));
    Ok(())
}

#[test]
fn a_variable_new_in_the_substitution_defines_a_triplet_others_can_use() -> Result<()> {
    let mut triplets = seeded()?;
    let changes = process_triplets_query(&mut triplets, "() (($new: 5 0 0) (72: $new 0 0))")?;
    assert_eq!(changes.len(), 2);
    let new = changes[0].after.unwrap().index;
    assert_eq!(triplets.get_triplet(72), Some(Triplet::new(72, new, 0, 0)));
    Ok(())
}

#[test]
fn a_matched_triplet_the_substitution_leaves_out_is_deleted() -> Result<()> {
    let mut triplets = seeded()?;
    process_triplets_query(&mut triplets, "((6: 5 5 5)) ((88: 5 0 0))")?;
    assert!(!triplets.is_triplet(6));
    assert!(triplets.is_triplet(88));
    Ok(())
}
