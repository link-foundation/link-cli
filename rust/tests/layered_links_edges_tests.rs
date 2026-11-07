//! The edges of the layers every store can stand under: a [`RemapLinks`] and
//! its maps, a [`UnionLinks`] and a [`NamedLinksStorage`], down to the answers
//! they give for addresses that are not theirs.

use anyhow::Result;
use link_cli::{
    AddressMap, AddressTable, Link, LinkError, LinkStorage, LinksStorage, NamedLinksStorage,
    NamedTypeLinks, OffsetAddressMap, RemapLinks, UnionLinks,
};

fn remap(offset: u32) -> RemapLinks<LinkStorage, OffsetAddressMap> {
    RemapLinks::new(LinkStorage::in_memory(false), OffsetAddressMap::new(offset))
}

fn panics(run: impl FnOnce() + std::panic::UnwindSafe) -> String {
    let payload = std::panic::catch_unwind(run).unwrap_err();
    payload
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_else(|| payload.downcast_ref::<&str>().unwrap_or(&"").to_string())
}

#[test]
fn an_offset_map_refuses_addresses_past_the_last() {
    let mut map = OffsetAddressMap::new(10);
    assert_eq!(map.to_outer(5).unwrap(), 15);
    assert_eq!(map.to_inner(15), Some(5));
    assert_eq!(map.to_inner(10), None);
    assert!(map
        .to_outer(u32::MAX - 10)
        .unwrap_err()
        .to_string()
        .contains("past the last"));
    assert!(map
        .to_outer(u32::MAX)
        .unwrap_err()
        .to_string()
        .contains("past the last"));
}

#[test]
fn an_address_table_keeps_each_address_mapped_once_and_skips_taken_ones() {
    assert!(AddressTable::new(0)
        .unwrap_err()
        .to_string()
        .contains("null link"));
    let mut table = AddressTable::new(100).unwrap();
    for (outer, inner) in [(0, 1), (1, 0)] {
        assert!(table
            .add(outer, inner)
            .unwrap_err()
            .to_string()
            .contains("null link"));
    }
    table.add(100, 1).unwrap();
    table.add(100, 1).unwrap();
    table.add(101, 7).unwrap();
    assert!(table
        .add(100, 2)
        .unwrap_err()
        .to_string()
        .contains("mapped to 1"));
    assert!(table
        .add(102, 1)
        .unwrap_err()
        .to_string()
        .contains("mapped from 100"));
    assert_eq!(table.to_inner(100), Some(1));
    assert_eq!(table.to_inner(999), None);
    assert_eq!(table.to_outer(1).unwrap(), 100);
    assert_eq!(table.to_outer(2).unwrap(), 102, "100 and 101 are taken");
    assert_eq!(table.pairs(), [(100, 1), (101, 7), (102, 2)]);
}

#[test]
fn a_remap_hands_out_its_parts_and_maps_every_answer() -> Result<()> {
    let mut links = remap(100);
    assert_eq!(links.map().offset, 100);
    let first = links.try_create(0, 0)?;
    assert_eq!(first, 101);
    assert_eq!(links.create(first, first), 102);
    assert_eq!(links.inner().all().len(), 2);
    assert_eq!(links.inner_mut().get_or_create(1, 1), 2);
    assert_eq!(links.to_inner(0), Some(0));
    assert_eq!(links.to_outer(0)?, 0);

    assert_eq!(
        links.all_links(),
        [Link::new(101, 0, 0), Link::new(102, 101, 101)]
    );
    assert_eq!(
        links.query_links(None, Some(101), None),
        [Link::new(102, 101, 101)]
    );
    assert!(links.query_links(Some(5), None, None).is_empty());
    assert!(links.query_links(None, Some(5), None).is_empty());
    assert!(links.query_links(None, None, Some(5)).is_empty());

    assert_eq!(links.search(101, 101), Some(102));
    assert_eq!(links.search(5, 101), None);
    assert_eq!(links.search(101, 5), None);
    assert_eq!(links.search(102, 102), None);
    assert_eq!(links.get_or_create(101, 101), 102);
    assert_eq!(links.get_or_create(102, 102), 103);
    assert_eq!(links.try_get_or_create(101, 101)?, 102);
    assert_eq!(links.try_get_or_create(103, 103)?, 104);

    assert!(links.reference_exists(101));
    assert!(!links.reference_exists(0));
    assert!(!links.reference_exists(5));
    assert!(!links.exists(0));
    assert!(!links.exists(5));
    assert_eq!(links.get_link(0), None);
    assert_eq!(links.get_link(5), None);
    assert_eq!(links.get_link(999), None);
    assert_eq!(links.claims(5), Some(false));
    assert_eq!(links.claims(500), Some(true));
    assert_eq!(links.next_created_addresses(2)?, [105, 106]);
    assert_eq!(links.ensure_created(110), 110);
    assert!(links.exists(110));
    links.save()?;
    let (inner, map) = links.into_inner();
    assert_eq!(map.offset, 100);
    assert!(inner.exists(10));
    Ok(())
}

#[test]
fn a_remap_refuses_writes_that_name_addresses_that_are_not_its_own() -> Result<()> {
    let mut links = remap(100);
    links.try_create(0, 0)?;
    let refused = |error: anyhow::Error, expected: &str| {
        assert!(error.to_string().contains(expected), "{error}");
    };
    refused(links.try_create(5, 0).unwrap_err(), "cannot refer to 5");
    refused(links.try_create(0, 5).unwrap_err(), "cannot refer to 5");
    refused(
        links.try_ensure_created(5).unwrap_err(),
        "cannot refer to 5",
    );
    refused(links.update(5, 0, 0).unwrap_err(), "5");
    refused(links.update(0, 0, 0).unwrap_err(), "0");
    refused(links.update(101, 5, 0).unwrap_err(), "cannot refer to 5");
    refused(links.update(101, 0, 5).unwrap_err(), "cannot refer to 5");
    refused(links.delete(5).unwrap_err(), "5");
    refused(links.set_name(5, "x").unwrap_err(), "5");
    Ok(())
}

#[test]
fn a_remap_panics_only_through_the_calls_that_cannot_answer_an_error() {
    assert!(panics(|| {
        remap(100).create(5, 0);
    })
    .contains("cannot refer to 5"));
    assert!(panics(|| {
        remap(100).ensure_created(5);
    })
    .contains("cannot refer to 5"));
    assert!(panics(|| {
        remap(100).get_or_create(5, 0);
    })
    .contains("cannot refer to 5"));
}

#[test]
fn a_remap_stops_at_an_address_it_cannot_show() {
    // Link 11 of the store below would read past the last address above.
    let mut links = remap(u32::MAX - 11);
    for _ in 0..10 {
        links.try_create(0, 0).unwrap();
    }
    let above = u32::MAX - 1;
    assert_eq!(links.get_link(above).unwrap().index, above);
    assert!(links.try_create(0, 0).is_err());
    assert_eq!(links.get_link(u32::MAX), None, "past the last address");
    let message = panics(std::panic::AssertUnwindSafe(|| {
        links.all_links();
    }));
    assert!(message.contains("past the last"), "{message}");
    assert!(links.next_created_addresses(1).is_err());
}

#[test]
fn a_remap_finds_names_under_its_own_addresses() -> Result<()> {
    let mut links = remap(100);
    let named = links.get_or_create_named("seen")?;
    assert_eq!(named, 101);
    assert_eq!(links.get_or_create_named("seen")?, 101);
    assert_eq!(links.get_link(101), Some(Link::new(101, 101, 101)));
    assert_eq!(links.get_by_name("seen")?, Some(101));
    assert_eq!(links.get_by_name("absent")?, None);
    assert_eq!(links.get_name(101)?.as_deref(), Some("seen"));
    assert_eq!(links.get_name(0)?, None);
    assert_eq!(links.get_name(5)?, None);
    links.remove_name(0)?;
    links.remove_name(5)?;
    links.remove_name(101)?;
    assert_eq!(links.get_name(101)?, None);
    Ok(())
}

fn union_of<'a>(first: &'a mut LinkStorage, second: &'a mut LinkStorage) -> Result<UnionLinks<'a>> {
    UnionLinks::new(vec![Box::new(first), Box::new(second)])
}

#[test]
fn a_union_needs_a_member_and_hands_them_out() -> Result<()> {
    assert!(UnionLinks::new(Vec::new())
        .err()
        .unwrap()
        .to_string()
        .contains("at least one network"));
    let (mut first, mut second) = (LinkStorage::in_memory(false), LinkStorage::in_memory(false));
    let mut union = union_of(&mut first, &mut second)?;
    assert_eq!(union.members().len(), 2);
    union.primary().try_create(0, 0)?;
    assert!(union.exists(1));
    Ok(())
}

#[test]
fn a_union_answers_from_the_member_that_has_the_address() -> Result<()> {
    let (mut first, mut second) = (LinkStorage::in_memory(false), LinkStorage::in_memory(false));
    for _ in 0..3 {
        second.create(0, 0);
    }
    second.update(2, 2, 2)?;
    first.create(1, 1);
    second.set_name(3, "kept");
    second.set_name(1, "shadowed");
    first.set_name(9, "unowned");
    let mut union = union_of(&mut first, &mut second)?;

    assert_eq!(
        union.try_ensure_created(3)?,
        3,
        "a later member has it already"
    );
    assert_eq!(union.owner_of(3), Some(1));
    assert_eq!(union.owner_of(0), None);

    assert_eq!(union.get_link(2), Some(Link::new(2, 2, 2)));
    assert_eq!(union.get_link(99), None);
    assert!(union.reference_exists(3));
    assert!(!union.reference_exists(99));
    assert_eq!(union.claims(99), Some(true));
    assert!(!union.next_created_addresses(1)?.is_empty());

    // A doublet of a later member, and one shadowed by an earlier member.
    assert_eq!(union.search(2, 2), Some(2));
    assert_eq!(union.search(1, 1), Some(1));
    assert_eq!(union.search(77, 77), None);
    assert_eq!(union.get_or_create(2, 2), 2);
    assert_eq!(union.get_or_create(77, 77), 2);
    assert_eq!(
        union.owner_of(2),
        Some(0),
        "the new link shadows the later member's"
    );

    assert_eq!(union.get_name(3)?.as_deref(), Some("kept"));
    assert_eq!(union.get_name(9)?.as_deref(), Some("unowned"));
    assert_eq!(union.get_name(99)?, None);
    assert_eq!(union.get_by_name("kept")?, Some(3));
    assert_eq!(union.get_by_name("absent")?, None);
    assert_eq!(
        union.get_by_name("shadowed")?,
        None,
        "its link 1 is shadowed"
    );
    union.set_name(3, "renamed")?;
    assert_eq!(union.get_by_name("renamed")?, Some(3));
    union.remove_name(3)?;
    assert_eq!(union.get_by_name("renamed")?, None);

    let made = union.get_or_create_named("made")?;
    assert_eq!(union.get_or_create_named("made")?, made);
    assert_eq!(union.get_link(made), Some(Link::new(made, made, made)));
    union.save()?;
    Ok(())
}

#[test]
fn a_new_primary_link_shadows_a_later_member_at_the_same_address() -> Result<()> {
    let (mut first, mut second) = (LinkStorage::in_memory(false), LinkStorage::in_memory(false));
    for _ in 0..3 {
        second.create(0, 0);
    }
    first.create(1, 1);
    let mut union = union_of(&mut first, &mut second)?;
    assert_eq!(union.ensure_created(4), 4);
    assert_eq!(union.owner_of(4), Some(0));
    let created = union.try_create(9, 9)?;
    assert!(
        created < 4,
        "the primary member reuses its own free address"
    );
    assert_eq!(union.owner_of(created), Some(0));
    assert_eq!(union.get_link(created), Some(Link::new(created, 9, 9)));
    Ok(())
}

#[test]
fn a_union_places_a_new_address_in_the_member_that_claims_it() -> Result<()> {
    let mut union = UnionLinks::new(vec![Box::new(remap(0)), Box::new(remap(1_000))])?;
    assert_eq!(union.ensure_created(1_005), 1_005);
    assert_eq!(
        union.owner_of(1_005),
        Some(0),
        "the first member claims every address"
    );
    let mut union = UnionLinks::new(vec![Box::new(remap(1_000)), Box::new(remap(0))])?;
    assert_eq!(union.try_ensure_created(5)?, 5);
    assert_eq!(union.owner_of(5), Some(1));
    Ok(())
}

#[test]
fn a_union_of_failing_members_answers_their_error() -> Result<()> {
    let mut union = UnionLinks::new(vec![Box::new(remap(100))])?;
    assert!(union.update(5, 0, 0).is_err());
    let message = panics(std::panic::AssertUnwindSafe(|| {
        union.ensure_created(5);
    }));
    assert!(message.contains("cannot refer to 5"), "{message}");
    Ok(())
}

#[test]
fn a_named_layer_works_as_a_storage_and_keeps_its_errors() -> Result<()> {
    let mut storage = NamedLinksStorage::new(remap(100));
    assert_eq!(storage.create_link(0, 0)?, 101);
    assert_eq!(storage.ensure_link_created(103)?, 103);
    assert!(storage.link_exists(103));
    assert!(!storage.link_exists(999));
    assert_eq!(storage.get_link(101), Some(Link::new(101, 0, 0)));
    // An update answers the row as it was.
    assert_eq!(storage.update_link(103, 101, 101)?, Link::new(103, 0, 0));
    assert_eq!(storage.search_link(101, 101), Some(103));
    assert_eq!(storage.get_or_create_link(101, 101)?, 103);
    assert_eq!(storage.query_links(None, Some(101), None).len(), 1);
    assert!(storage.all_links().len() >= 2);

    let mut changes = Vec::new();
    storage.update_link_observed(103, 101, 0, &mut |before, after| {
        changes.push((before, after));
    })?;
    storage.delete_link_observed(103, &mut |before, after| changes.push((before, after)))?;
    assert_eq!(
        changes[0],
        (Link::new(103, 101, 101), Link::new(103, 101, 0))
    );
    assert_eq!(
        changes.last().unwrap().1,
        Link::new(0, 0, 0),
        "the row is gone"
    );
    assert_eq!(storage.delete_link(101)?.index, 101);
    storage.flush()?;

    // The layer's own LinkError stays one; anything else reads as a storage error.
    assert!(matches!(
        storage.delete_link(5),
        Err(LinkError::NotFound { .. })
    ));
    match storage.create_link(5, 0) {
        Err(LinkError::StorageError(message)) => assert!(message.contains("cannot refer to 5")),
        other => panic!("{other:?}"),
    }
    assert_eq!(storage.links_mut().map().offset, 100);
    assert_eq!(storage.into_inner().map().offset, 100);
    Ok(())
}
