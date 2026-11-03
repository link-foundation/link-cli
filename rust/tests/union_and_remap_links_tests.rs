//! Networks joined into one and networks under other addresses (issue #149):
//! [`UnionLinks`], [`RemapLinks`] and [`LinksExternalIds`]. The C# twin is
//! `csharp/Foundation.Data.Doublets.Cli.Tests/UnionAndRemapLinksTests.cs`,
//! case for case.

use anyhow::Result;
use link_cli::{
    AddressTable, LinkChange, LinkStorage, LinksExternalIds, NamedTypeLinks, OffsetAddressMap,
    RemapLinks, UnionLinks,
};

#[path = "common/union_world.rs"]
mod union_world;

use union_world::{all, all_of, make};

fn with_union(test: impl FnOnce(&mut UnionLinks<'_>) -> Result<()>) -> Result<()> {
    let mut local = LinkStorage::in_memory(false);
    let mut server = LinkStorage::in_memory(false);
    let mut union = UnionLinks::new(vec![Box::new(&mut local), Box::new(&mut server)])?;
    test(&mut union)
}

/// The members of a union, the local one and the server, to write below it.
fn member<'u>(union: &'u mut UnionLinks<'_>, at: usize) -> &'u mut dyn NamedTypeLinks {
    &mut *union.members_mut()[at]
}

#[test]
fn union_reads_the_links_of_every_member() -> Result<()> {
    with_union(|union| {
        make(member(union, 1), 1, 1)?;
        make(member(union, 1), 2, 2)?;
        make(member(union, 1), 2, 1)?;
        let local1 = make(member(union, 0), 3, 3)?;
        assert_eq!(vec![(local1, 3, 3), (2, 2, 2), (3, 2, 1)], all(union));
        assert_eq!(3, union.all_links().len());
        assert!(union.exists(3));
        assert_eq!(vec![(2, 2, 2), (3, 2, 1)], all_of(union, Some(2), None));
        assert_eq!(vec![(local1, 3, 3)], all_of(union, None, Some(3)));
        assert_eq!(Some(0), union.owner_of(1));
        assert_eq!(Some(1), union.owner_of(2));
        assert_eq!(None, union.owner_of(99));
        Ok(())
    })
}

#[test]
fn union_first_member_shadows_the_rest() -> Result<()> {
    with_union(|union| {
        make(member(union, 1), 1, 1)?;
        make(member(union, 0), 2, 2)?;
        assert_eq!(vec![(1, 2, 2)], all(union));
        assert_eq!(1, union.all_links().len());
        assert!(all_of(union, Some(1), None).is_empty());
        Ok(())
    })
}

#[test]
fn union_creates_in_the_primary_and_writes_where_the_link_lives() -> Result<()> {
    with_union(|union| {
        make(member(union, 1), 1, 1)?;
        make(member(union, 1), 2, 2)?;
        let created = union.create(0, 0);
        union.update(created, 2, 2)?;
        assert_eq!(vec![(created, 2, 2)], all(member(union, 0)));
        union.update(2, 2, created)?;
        assert_eq!(vec![(1, 1, 1), (2, 2, created)], all(member(union, 1)));
        union.delete(2)?;
        assert_eq!(vec![(1, 1, 1)], all(member(union, 1)));
        assert_eq!(vec![(created, 2, 2)], all(union));
        Ok(())
    })
}

#[test]
fn union_refuses_writes_to_many_links_at_once() -> Result<()> {
    with_union(|union| {
        // A write names one link; the null link is none.
        assert!(union.delete(0).is_err());
        assert!(union.update(0, 2, 2).is_err());
        Ok(())
    })
}

#[test]
fn union_finds_names_in_member_order() -> Result<()> {
    with_union(|union| {
        make(member(union, 1), 1, 1)?;
        make(member(union, 1), 2, 2)?;
        let type_ = make(member(union, 0), 1, 1)?;
        member(union, 1).set_name(2, "Note")?;
        member(union, 1).set_name(1, "Shadowed")?;
        assert_eq!(Some(2), union.get_by_name("Note")?);
        assert_eq!(Some("Note".to_string()), union.get_name(2)?);
        assert_eq!(None, union.get_by_name("Shadowed")?);
        member(union, 0).set_name(type_, "Note")?;
        assert_eq!(Some(type_), union.get_by_name("Note")?);
        union.set_name(2, "Remote")?;
        assert_eq!(Some(2), member(union, 1).get_by_name("Remote")?);
        assert_eq!(None, member(union, 0).get_by_name("Remote")?);
        union.remove_name(2)?;
        assert_eq!(None, member(union, 1).get_name(2)?);
        Ok(())
    })
}

fn with_remap(
    test: impl FnOnce(&mut RemapLinks<LinkStorage, OffsetAddressMap>) -> Result<()>,
) -> Result<()> {
    let mut remap = RemapLinks::new(LinkStorage::in_memory(false), OffsetAddressMap::new(1000));
    test(&mut remap)
}

#[test]
fn remap_moves_every_address_by_an_offset() -> Result<()> {
    with_remap(|remap| {
        let point = make(remap.inner_mut(), 1, 1)?;
        assert_eq!(vec![(1001, 1001, 1001)], all(remap));
        let created = remap.create(0, 0);
        assert_eq!(1002, created);
        remap.update(created, 1001, created)?;
        assert_eq!(
            vec![(point, point, point), (2, 1, 2)],
            all(remap.inner_mut())
        );
        assert_eq!(
            vec![(1002, 1001, 1002)],
            all_of(remap, Some(1001), Some(1002))
        );
        remap.delete(1002)?;
        assert_eq!(vec![(point, point, point)], all(remap.inner_mut()));
        Ok(())
    })
}

#[test]
fn remap_finds_nothing_and_writes_nothing_outside_its_network() -> Result<()> {
    with_remap(|remap| {
        let point = make(remap.inner_mut(), 1, 1)?;
        assert!(all_of(remap, Some(1), None).is_empty());
        assert!(!remap.exists(1));
        assert!(all_of(remap, Some(5), None).is_empty());
        let error = remap.update(1001, 5, 1001).unwrap_err();
        assert!(error.to_string().contains("not of this network"), "{error}");
        let error = remap.delete(5).unwrap_err();
        assert!(error.to_string().starts_with("Link not found"), "{error}");
        assert_eq!(vec![(point, point, point)], all(remap.inner_mut()));
        Ok(())
    })
}

#[test]
fn remap_reports_changes_under_its_addresses() -> Result<()> {
    with_remap(|remap| {
        let created = remap.create(0, 0);
        assert_eq!(1001, created);
        let mut changes: Vec<LinkChange> = Vec::new();
        remap.update_observed(1001, 1001, 1001, &mut |before, after| {
            changes.push((Some(before), Some(after)))
        })?;
        let (before, after) = changes.last().copied().expect("a change");
        assert_eq!((1001, 0, 0), before.map(triple).expect("before"));
        assert_eq!((1001, 1001, 1001), after.map(triple).expect("after"));
        Ok(())
    })
}

fn triple(link: link_cli::Link) -> (u32, u32, u32) {
    (link.index, link.source, link.target)
}

#[test]
fn remap_names_under_its_addresses() -> Result<()> {
    with_remap(|remap| {
        make(remap.inner_mut(), 1, 1)?;
        remap.set_name(1001, "Type")?;
        assert_eq!(Some(1), remap.inner_mut().get_by_name("Type")?);
        assert_eq!(Some(1001), remap.get_by_name("Type")?);
        assert_eq!(Some("Type".to_string()), remap.get_name(1001)?);
        assert_eq!(None, remap.get_name(1)?);
        remap.remove_name(1001)?;
        assert_eq!(None, remap.get_by_name("Type")?);
        Ok(())
    })
}

#[test]
fn address_table_gives_addresses_in_the_order_they_are_seen() -> Result<()> {
    let mut table = AddressTable::new(500)?;
    table.add(900, 2)?;
    let mut remap = RemapLinks::new(LinkStorage::in_memory(false), table);
    make(remap.inner_mut(), 1, 1)?;
    make(remap.inner_mut(), 1, 2)?;
    assert_eq!(vec![(500, 500, 500), (900, 500, 900)], all(&mut remap));
    let created = remap.create(0, 0);
    assert_eq!(501, created);
    let (_, mut table) = remap.into_inner();
    assert_eq!(vec![(500, 1), (501, 3), (900, 2)], table.pairs());
    assert!(table.add(500, 7).is_err());
    assert!(table.add(600, 1).is_err());
    Ok(())
}

#[test]
fn union_of_remapped_networks_shadows_nothing() -> Result<()> {
    let mut local = LinkStorage::in_memory(false);
    let mut server = LinkStorage::in_memory(false);
    make(&mut local, 1, 1)?;
    make(&mut server, 1, 1)?;
    let remote = (1 << 24) + 1;
    let mut union = UnionLinks::new(vec![
        Box::new(&mut local),
        Box::new(RemapLinks::new(&mut server, OffsetAddressMap::new(1 << 24))),
    ])?;
    assert_eq!(vec![(1, 1, 1), (remote, remote, remote)], all(&mut union));
    let typed = make(&mut union, remote, 1)?;
    assert_eq!(
        vec![(typed, remote, 1)],
        all_of(member(&mut union, 0), Some(remote), None)
    );
    union.update(remote, remote, remote)?;
    assert_eq!(Some(1), union.owner_of(remote));
    assert!(union.update(remote, remote, typed).is_err());
    Ok(())
}

fn external_ids_bind_ids_of_other_systems_to_links(table_apart: bool) -> Result<()> {
    let mut data = LinkStorage::in_memory(false);
    let mut apart = LinkStorage::in_memory(false);
    let ids = LinksExternalIds::new("uuid")?;
    const FIRST: &str = "0b4a3c1e-8f6d-4d2a-9a51-7c1f2e3d4b5a";
    const SECOND: &str = "7e9d1a2b-3c4d-4e5f-8a9b-0c1d2e3f4a5b";
    // The store of the table and, when it is apart, the data.
    macro_rules! table {
        () => {
            if table_apart {
                &mut apart as &mut dyn NamedTypeLinks
            } else {
                &mut data as &mut dyn NamedTypeLinks
            }
        };
    }
    macro_rules! ensure {
        ($id:expr) => {
            if table_apart {
                ids.ensure(&mut apart, Some(&mut data), $id)?
            } else {
                ids.ensure(&mut data, None, $id)?
            }
        };
    }
    assert_eq!(None, ids.find(table!(), FIRST)?);
    let link = ensure!(FIRST);
    assert_eq!(link, ensure!(FIRST));
    assert_eq!(Some(link), ids.find(table!(), FIRST)?);
    assert_eq!(Some(FIRST.to_string()), ids.id_of(table!(), link)?);
    assert_eq!(Some((link, link, link)), data.get_link(link).map(triple));
    ids.bind(table!(), SECOND, link)?;
    assert_eq!(None, ids.find(table!(), FIRST)?);
    assert_eq!(Some(SECOND.to_string()), ids.id_of(table!(), link)?);
    let other = make(&mut data, link, link)?;
    ids.bind(table!(), SECOND, other)?;
    assert_eq!(None, ids.id_of(table!(), link)?);
    assert_eq!(Some(other), ids.find(table!(), SECOND)?);
    assert!(ids.unbind(table!(), SECOND)?);
    assert!(!ids.unbind(table!(), SECOND)?);
    assert_eq!(None, ids.id_of(table!(), other)?);
    assert!(LinksExternalIds::new("a:b").is_err());
    for address in 1..=6 {
        data.ensure_created(address);
        ids.bind(table!(), &format!("n{address}"), address)?;
    }
    for address in 1..=6 {
        assert_eq!(Some(address), ids.find(table!(), &format!("n{address}"))?);
        assert_eq!(Some(format!("n{address}")), ids.id_of(table!(), address)?);
    }
    Ok(())
}

#[test]
fn external_ids_bind_ids_of_other_systems_to_links_in_the_data() -> Result<()> {
    external_ids_bind_ids_of_other_systems_to_links(false)
}

#[test]
fn external_ids_bind_ids_of_other_systems_to_links_in_a_table_apart() -> Result<()> {
    external_ids_bind_ids_of_other_systems_to_links(true)
}
