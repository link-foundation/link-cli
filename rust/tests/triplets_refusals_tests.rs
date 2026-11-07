//! Replies a client of the triplet protocol must not believe, and a triplet
//! store behind a permissions view whose subject may only refer to some links.

use anyhow::Result;
use link_cli::protocol::parse_document;
use link_cli::protocol::triplets_operations::{
    matches_triplet, parse_triplet, parse_triplet_changes, restriction_of_query,
};
use link_cli::{
    AccessPolicy, Link, LinkStorage, NamedTypeLinks, PermissionMarkers, PermissionsDecorator,
    Triplet, TripletQuery, Triplets, TripletsOverDoublets,
};
use std::collections::HashSet;

fn refused(text: &str) -> String {
    parse_triplet_changes(&parse_document(text).unwrap())
        .expect_err(text)
        .to_string()
}

#[test]
fn a_change_needs_both_its_sides() {
    assert!(refused("((1: 1 1 1))").contains("expected a change reply"));
    assert!(refused("(() (a b c) ())").contains("expected a change reply"));
    assert!(refused("(() x)").contains("expected a change side"));
    // A side that is the single named link itself lost its wrapper.
    let named = parse_triplet_changes(&parse_document("((5: 1 2 3) ())").unwrap()).unwrap();
    assert_eq!(named[0].before, Some(Triplet::new(5, 1, 2, 3)));
    assert_eq!(named[0].after, None);
}

#[test]
fn a_triplet_has_an_index_and_three_parts() {
    for text in ["(1 2 3)", "(5: 1 2)", "x"] {
        let document = parse_document(text).unwrap();
        let refusal = parse_triplet(&document[0]).expect_err(text).to_string();
        assert!(refusal.contains("expected (index: type from to)"), "{text}");
    }
}

#[test]
fn a_restriction_longer_than_a_triplet_matches_nothing() {
    let triplet = Triplet::new(1, 2, 3, 4);
    let part = restriction_of_query(TripletQuery::by_from(3));
    assert!(matches_triplet(&triplet, &part));
    let mut long = part.clone();
    long.push(part[0]);
    assert!(!matches_triplet(&triplet, &long));
}

#[test]
fn markers_can_be_chosen_by_address() -> Result<()> {
    let mut links = LinkStorage::in_memory(false).with_indexed_lookups();
    let pair = links.try_create(0, 0)?;
    let nothing = links.try_create(0, 0)?;
    let mut triplets = TripletsOverDoublets::new(links, pair, nothing);
    assert_eq!(triplets.triplet_markers().pair, pair);
    assert_eq!(triplets.triplet_markers().nothing, nothing);
    let point = triplets.create_triplet_point()?;
    assert!(triplets.is_triplet(point));
    Ok(())
}

/// Everything may be written and read; only some links may be referred to.
struct OnlySomeReferences(HashSet<u32>);

impl AccessPolicy for OnlySomeReferences {
    fn markers(&self) -> PermissionMarkers {
        PermissionMarkers {
            allow_read: 1,
            allow_write: 2,
            anything: 3,
            allow_reference: 4,
        }
    }
    fn can_read(&mut self, _: &mut dyn NamedTypeLinks, _: u32, link: &Link) -> bool {
        self.0.contains(&link.index)
    }
    fn can_write(&mut self, _: &mut dyn NamedTypeLinks, _: u32, _: &Link) -> bool {
        true
    }
    fn can_create(&mut self, _: &mut dyn NamedTypeLinks, _: u32) -> bool {
        true
    }
    fn can_reference(&mut self, _: &mut dyn NamedTypeLinks, _: u32, link: &Link) -> bool {
        self.0.contains(&link.index)
    }
}

fn is_not_found<T: std::fmt::Debug>(result: Result<T>) -> bool {
    result
        .unwrap_err()
        .to_string()
        .starts_with("Link not found")
}

#[test]
fn a_view_of_triplets_answers_alike_for_what_it_may_not_refer_to() -> Result<()> {
    let mut store = TripletsOverDoublets::with_named_markers(
        LinkStorage::in_memory(false).with_indexed_lookups(),
    )?;
    let markers = store.triplet_markers();
    let hidden = store.create_triplet_point()?;
    let visible_type = store.create_triplet_point()?;
    let own = store.create_triplet(visible_type, 0, 0)?;
    let mine = HashSet::from([visible_type, own, 90]);
    let mut view = PermissionsDecorator::new(store, OnlySomeReferences(mine), 0);

    assert_eq!(view.triplet_markers(), markers);
    // A hidden type and a free address both answer as a missing link.
    assert!(is_not_found(view.create_triplet(hidden, 0, 0)));
    assert!(is_not_found(view.update_triplet(own, hidden, 0, 0)));
    assert!(is_not_found(view.create_triplet_point_at(91)));

    // An address the subject may name can be taken once.
    view.create_triplet_point_at(90)?;
    assert!(view.is_triplet(90));
    let refusal = view.create_triplet_point_at(90).unwrap_err().to_string();
    assert!(refusal.contains("exists"), "{refusal}");
    Ok(())
}
