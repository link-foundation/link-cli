//! Triplets stored in doublets (issue #151). The C# twin is
//! `csharp/Foundation.Data.Doublets.Cli.Tests/TripletsOverDoubletsTests.cs`,
//! case for case, with the same pseudo-random sequence in the parity test.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use link_cli::{
    import_triplets_lino, process_triplets_query, triplets_to_lino, DoubletRole, Link, LinkStorage,
    NamedTypeLinks, NamedTypesDecorator, Triplet, TripletChange, TripletQuery, Triplets,
    TripletsOverDoublets, Types, TypesDecorator, NOTHING_MARKER_NAME, PAIR_MARKER_NAME,
};
use tempfile::NamedTempFile;

type Store = TripletsOverDoublets<LinkStorage>;

fn store() -> Result<Store> {
    // Indexed lookups, as the trees of UnitedMemoryLinks in C#, keep the
    // costs of docs/TRIPLETS.md.
    TripletsOverDoublets::with_named_markers(LinkStorage::in_memory(false).with_indexed_lookups())
}

fn doublet(links: &mut impl NamedTypeLinks, id: u32) -> Link {
    links.get_link(id).expect("the doublet exists")
}

/// Every doublet is a marker, a triplet, or a wrapper or pair still in use.
fn assert_no_orphans(triplets: &mut impl Triplets, allow_foreign: bool) {
    for link in triplets.all_links() {
        let role = triplets
            .role_of(link.index)
            .expect("a stored doublet has a role");
        if role.is_auxiliary() {
            assert!(
                triplets.doublet_usages(link.index) > 0,
                "orphan {role:?} {link:?}"
            );
        } else if role == DoubletRole::Foreign {
            assert!(allow_foreign, "a doublet that is no triplet: {link:?}");
        }
    }
}

/// `Type`, `Person`, `knows`, `alice`, `bob` and `carol`.
struct People {
    root: u32,
    person: u32,
    knows: u32,
    alice: u32,
    bob: u32,
    carol: u32,
}

fn people(triplets: &mut impl Triplets) -> Result<People> {
    let root = triplets.create_triplet_point()?;
    let person = triplets.create_triplet(root, 0, 0)?;
    let knows = triplets.create_triplet(root, 0, 0)?;
    let alice = triplets.create_triplet(person, 0, 0)?;
    let bob = triplets.create_triplet(person, 0, 0)?;
    let carol = triplets.create_triplet(person, 0, 0)?;
    Ok(People {
        root,
        person,
        knows,
        alice,
        bob,
        carol,
    })
}

#[test]
fn implements_named_type_links_and_triplets() -> Result<()> {
    fn layer<L: NamedTypeLinks + Triplets>(_: &L) {}
    let triplets = store()?;
    layer(&triplets);
    Ok(())
}

#[test]
fn markers_are_named_points() -> Result<()> {
    let mut triplets = store()?;
    let markers = triplets.triplet_markers();
    assert_eq!(Some(markers.pair), triplets.get_by_name(PAIR_MARKER_NAME)?);
    assert_eq!(
        Some(markers.nothing),
        triplets.get_by_name(NOTHING_MARKER_NAME)?
    );
    for marker in [markers.pair, markers.nothing] {
        assert_eq!(
            Link::new(marker, marker, marker),
            doublet(&mut triplets, marker)
        );
        assert_eq!(Some(DoubletRole::Marker), triplets.role_of(marker));
        assert!(!triplets.is_triplet(marker));
    }
    // The same markers are found again.
    let again = TripletsOverDoublets::with_named_markers(triplets.into_inner())?;
    assert_eq!(markers, again.triplet_markers());
    Ok(())
}

#[test]
fn stores_nodes_and_links() -> Result<()> {
    let mut triplets = store()?;
    let markers = triplets.triplet_markers();
    let p = people(&mut triplets)?;
    // A node (i: T 0 0) is the doublet (i: T i), a value of #148.
    assert_eq!(
        Link::new(p.root, p.root, p.root),
        doublet(&mut triplets, p.root)
    );
    assert_eq!(
        Link::new(p.alice, p.person, p.alice),
        doublet(&mut triplets, p.alice)
    );
    assert_eq!(
        Some(Triplet::new(p.alice, p.person, 0, 0)),
        triplets.get_triplet(p.alice)
    );
    assert_eq!(
        Some(Triplet::new(p.root, p.root, 0, 0)),
        triplets.get_triplet(p.root)
    );

    let link = triplets.create_triplet(p.knows, p.alice, p.bob)?;
    assert_eq!(
        Some(Triplet::new(link, p.knows, p.alice, p.bob)),
        triplets.get_triplet(link)
    );
    // (link: knows pair), pair = (wrapper bob), wrapper = (Pair alice).
    let outer = doublet(&mut triplets, link);
    assert_eq!(p.knows, outer.source);
    let pair = doublet(&mut triplets, outer.target);
    assert_eq!(p.bob, pair.target);
    assert_eq!(Some(DoubletRole::Pair), triplets.role_of(pair.index));
    let wrapper = doublet(&mut triplets, pair.source);
    assert_eq!(Link::new(wrapper.index, markers.pair, p.alice), wrapper);
    assert_eq!(Some(DoubletRole::Wrapper), triplets.role_of(wrapper.index));
    assert_eq!(None, triplets.role_of(10_000));
    assert_eq!(None, triplets.get_triplet(wrapper.index));
    assert_eq!(None, triplets.get_triplet(0));
    assert_eq!(11, triplets.all_links().len());
    assert_no_orphans(&mut triplets, false);
    Ok(())
}

#[test]
fn zero_parts_are_nothing() -> Result<()> {
    let mut triplets = store()?;
    let markers = triplets.triplet_markers();
    let p = people(&mut triplets)?;
    let from_only = triplets.create_triplet(p.knows, p.alice, 0)?;
    let to_only = triplets.create_triplet(p.knows, 0, p.bob)?;
    assert_eq!(
        Some(Triplet::new(from_only, p.knows, p.alice, 0)),
        triplets.get_triplet(from_only)
    );
    assert_eq!(
        Some(Triplet::new(to_only, p.knows, 0, p.bob)),
        triplets.get_triplet(to_only)
    );
    let outer = doublet(&mut triplets, to_only);
    let pair = doublet(&mut triplets, outer.target);
    let wrapper = doublet(&mut triplets, pair.source);
    assert_eq!(markers.nothing, wrapper.target);
    assert_eq!(
        vec![Triplet::new(to_only, p.knows, 0, p.bob)],
        triplets.each_triplet(TripletQuery::new(None, Some(p.knows), Some(0), None))
    );
    assert_eq!(
        vec![Triplet::new(from_only, p.knows, p.alice, 0)],
        triplets.each_triplet(TripletQuery::new(None, None, Some(p.alice), Some(0)))
    );
    // Nodes have neither: an empty from finds them as well.
    assert_eq!(7, triplets.count_triplets(TripletQuery::by_from(0)));
    assert_eq!(7, triplets.count_triplets(TripletQuery::by_to(0)));
    assert_eq!(
        6,
        triplets.count_triplets(TripletQuery::new(None, None, Some(0), Some(0)))
    );
    assert_no_orphans(&mut triplets, false);
    Ok(())
}

#[test]
fn duplicates_get_their_own_address() -> Result<()> {
    let mut triplets = store()?;
    let p = people(&mut triplets)?;
    let doublets = triplets.all_links().len();
    let first = triplets.create_triplet(p.knows, p.alice, p.bob)?;
    let second = triplets.create_triplet(p.knows, p.alice, p.bob)?;
    let third = triplets.create_triplet(p.knows, p.alice, p.bob)?;
    assert_ne!(first, second);
    assert_ne!(second, third);
    // The second and third go through duplicate pairs (pair i).
    let pair = doublet(&mut triplets, first).target;
    let outer = doublet(&mut triplets, second);
    let duplicate = doublet(&mut triplets, outer.target);
    assert_eq!(Link::new(duplicate.index, pair, second), duplicate);
    assert_eq!(
        Some(DoubletRole::DuplicatePair),
        triplets.role_of(duplicate.index)
    );
    let expected = vec![
        Triplet::new(first, p.knows, p.alice, p.bob),
        Triplet::new(second, p.knows, p.alice, p.bob),
        Triplet::new(third, p.knows, p.alice, p.bob),
    ];
    assert_eq!(
        expected,
        triplets.each_triplet(TripletQuery::by_from(p.alice))
    );
    assert_eq!(expected, triplets.each_triplet(TripletQuery::by_to(p.bob)));
    assert_eq!(
        expected,
        triplets.each_triplet(TripletQuery::new(
            None,
            Some(p.knows),
            Some(p.alice),
            Some(p.bob)
        ))
    );
    assert_eq!(
        3,
        triplets.count_triplets(TripletQuery::new(None, None, None, Some(p.bob)))
    );

    triplets.delete_triplet(first)?;
    assert_eq!(
        expected[1..].to_vec(),
        triplets.each_triplet(TripletQuery::by_from(p.alice))
    );
    assert_no_orphans(&mut triplets, false);
    triplets.delete_triplet(third)?;
    triplets.delete_triplet(second)?;
    assert_eq!(doublets, triplets.all_links().len());
    assert_no_orphans(&mut triplets, false);
    Ok(())
}

#[test]
fn update_keeps_the_address_and_collects_the_old_pair() -> Result<()> {
    let mut triplets = store()?;
    let p = people(&mut triplets)?;
    let doublets = triplets.all_links().len();
    let link = triplets.create_triplet(p.knows, p.alice, p.bob)?;
    let old_pair = doublet(&mut triplets, link).target;
    let before = triplets.update_triplet(link, p.knows, p.alice, p.carol)?;
    assert_eq!(Triplet::new(link, p.knows, p.alice, p.bob), before);
    assert_eq!(
        Some(Triplet::new(link, p.knows, p.alice, p.carol)),
        triplets.get_triplet(link)
    );
    assert!(!triplets.exists(old_pair), "the old pair is collected");
    assert_eq!(doublets + 3, triplets.all_links().len());

    // A new type and new parts, then a node and back: the address stays.
    triplets.update_triplet(link, p.person, p.bob, p.alice)?;
    assert_eq!(
        Some(Triplet::new(link, p.person, p.bob, p.alice)),
        triplets.get_triplet(link)
    );
    triplets.update_triplet(link, p.person, 0, 0)?;
    assert_eq!(
        Some(Triplet::new(link, p.person, 0, 0)),
        triplets.get_triplet(link)
    );
    assert_eq!(doublets + 1, triplets.all_links().len());
    triplets.update_triplet(link, link, link, link)?;
    assert_eq!(
        Some(Triplet::new(link, link, link, link)),
        triplets.get_triplet(link)
    );
    assert_no_orphans(&mut triplets, false);
    triplets.update_triplet(link, p.knows, p.alice, p.bob)?;
    assert_eq!(doublets + 3, triplets.all_links().len());
    assert_no_orphans(&mut triplets, false);
    let unchanged = triplets.update_triplet(link, p.knows, p.alice, p.bob)?;
    assert_eq!(Triplet::new(link, p.knows, p.alice, p.bob), unchanged);
    assert_eq!(p.root, triplets.get_triplet(p.person).unwrap().type_id);
    Ok(())
}

#[test]
fn a_shared_pair_is_kept_while_it_is_used() -> Result<()> {
    let mut triplets = store()?;
    let p = people(&mut triplets)?;
    let knows = triplets.create_triplet(p.knows, p.alice, p.bob)?;
    let met = triplets.create_triplet(p.person, p.alice, p.bob)?;
    let pair = doublet(&mut triplets, knows).target;
    assert_eq!(
        pair,
        doublet(&mut triplets, met).target,
        "one pair, two triplets"
    );
    triplets.update_triplet(knows, p.knows, p.alice, p.carol)?;
    assert!(triplets.exists(pair), "the pair is still used");
    triplets.delete_triplet(met)?;
    assert!(!triplets.exists(pair));
    assert_no_orphans(&mut triplets, false);
    // An update into a slot another triplet holds takes a duplicate pair, and
    // an update out of it collects the duplicate pair.
    let first = triplets.create_triplet(p.knows, p.bob, p.carol)?;
    let second = triplets.create_triplet(p.person, p.bob, p.carol)?;
    triplets.update_triplet(second, p.knows, p.bob, p.carol)?;
    let outer = doublet(&mut triplets, second);
    assert_eq!(
        Some(DoubletRole::DuplicatePair),
        triplets.role_of(outer.target)
    );
    assert_eq!(3, triplets.count_triplets(TripletQuery::by_to(p.carol)));
    triplets.update_triplet(second, p.person, p.bob, p.carol)?;
    assert_eq!(
        doublet(&mut triplets, first).target,
        doublet(&mut triplets, second).target
    );
    assert_no_orphans(&mut triplets, false);
    Ok(())
}

#[test]
fn delete_cascades_to_the_triplets_that_use_it() -> Result<()> {
    let mut triplets = store()?;
    let p = people(&mut triplets)?;
    let markers_and_nodes = triplets.all_links().len();
    let ab = triplets.create_triplet(p.knows, p.alice, p.bob)?;
    let bc = triplets.create_triplet(p.knows, p.bob, p.carol)?;
    let about = triplets.create_triplet(p.knows, ab, bc)?;
    let deleted = triplets.delete_triplet(p.alice)?;
    let indices: Vec<u32> = deleted.iter().map(|triplet| triplet.index).collect();
    assert_eq!(vec![p.alice, ab, about], indices);
    assert!(triplets.is_triplet(bc));
    assert_no_orphans(&mut triplets, false);
    // A cycle: two triplets that use each other.
    let first = triplets.create_triplet(p.knows, p.bob, 0)?;
    let second = triplets.create_triplet(p.knows, first, 0)?;
    triplets.update_triplet(first, p.knows, second, 0)?;
    triplets.delete_triplet(second)?;
    assert!(!triplets.exists(first));
    triplets.delete_triplet(bc)?;
    assert_eq!(markers_and_nodes - 1, triplets.all_links().len());
    assert_no_orphans(&mut triplets, false);
    // The root takes every triplet with it.
    triplets.delete_triplet(p.root)?;
    assert_eq!(2, triplets.all_links().len(), "only the markers are left");
    Ok(())
}

#[test]
fn refuses_parts_that_are_not_triplets() -> Result<()> {
    let mut triplets = store()?;
    let markers = triplets.triplet_markers();
    let p = people(&mut triplets)?;
    let link = triplets.create_triplet(p.knows, p.alice, p.bob)?;
    let pair = doublet(&mut triplets, link).target;
    assert!(triplets.create_triplet(0, 0, 0).is_err());
    assert!(triplets.create_triplet(markers.pair, 0, 0).is_err());
    assert!(triplets.create_triplet(pair, 0, 0).is_err());
    assert!(triplets.create_triplet(p.knows, 10_000, 0).is_err());
    assert!(triplets
        .create_triplet(p.knows, 0, markers.nothing)
        .is_err());
    assert!(triplets.update_triplet(pair, p.knows, 0, 0).is_err());
    assert!(triplets.update_triplet(link, 0, 0, 0).is_err());
    assert!(triplets.delete_triplet(pair).is_err());
    assert!(triplets.delete_triplet(10_000).is_err());
    assert_eq!(
        Some(Triplet::new(link, p.knows, p.alice, p.bob)),
        triplets.get_triplet(link)
    );
    Ok(())
}

/// The type hierarchy of #148 reads triplet nodes as values of their types,
/// and its `Extends` wiring is not mistaken for triplets.
#[test]
fn works_with_the_type_hierarchy() -> Result<()> {
    let types = TypesDecorator::with_named_markers(LinkStorage::in_memory(false))?;
    let mut triplets = TripletsOverDoublets::with_named_markers(types)?;
    let root = triplets.links().markers().root;
    assert_eq!(
        Some(Triplet::new(root, root, 0, 0)),
        triplets.get_triplet(root)
    );
    let person = triplets.links_mut().define_root()?;
    let public = triplets.links_mut().define_root()?;
    let admin = triplets.links_mut().define(person, &[public])?;
    let boss = triplets.create_triplet(admin, 0, 0)?;
    assert!(triplets.links_mut().is_a(boss, person));
    assert!(triplets.links_mut().is_a(boss, public));
    assert_eq!(
        vec![
            Triplet::new(person, root, 0, 0),
            Triplet::new(public, root, 0, 0)
        ],
        triplets
            .each_triplet(TripletQuery::by_type(root))
            .into_iter()
            .filter(|triplet| triplet.index != root && triplet.index > public - 2)
            .collect::<Vec<_>>()
    );
    let knows = triplets.create_triplet(root, 0, 0)?;
    let link = triplets.create_triplet(knows, boss, admin)?;
    assert_eq!(
        vec![Triplet::new(link, knows, boss, admin)],
        triplets.each_triplet(TripletQuery::by_to(admin))
    );
    assert_no_orphans(&mut triplets, true);
    Ok(())
}

/// The same pseudo-random sequence in C# and Rust: a 64-bit LCG.
struct Random(u64);

impl Random {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) % bound as u64) as usize
    }
}

/// A triplet store kept in a map: what the decorator must agree with.
#[derive(Default)]
struct ReferenceStore(BTreeMap<u32, (u32, u32, u32)>);

impl ReferenceStore {
    fn each(&self, query: TripletQuery) -> Vec<Triplet> {
        self.0
            .iter()
            .map(|(&index, &(type_id, from, to))| Triplet::new(index, type_id, from, to))
            .filter(|triplet| query.matches(triplet))
            .collect()
    }

    fn delete(&mut self, index: u32) {
        let mut pending = vec![index];
        while let Some(deleted) = pending.pop() {
            if self.0.remove(&deleted).is_none() {
                continue;
            }
            for (&user, &(type_id, from, to)) in &self.0 {
                if type_id == deleted || from == deleted || to == deleted {
                    pending.push(user);
                }
            }
        }
    }
}

#[test]
fn each_agrees_with_a_triplet_store_on_a_thousand_triplets() -> Result<()> {
    let mut triplets = store()?;
    let mut reference = ReferenceStore::default();
    let mut random = Random(151);
    let root = triplets.create_triplet_point()?;
    reference.0.insert(root, (root, 0, 0));
    let mut protected = BTreeSet::from([root]);
    for _ in 0..10 {
        let type_id = triplets.create_triplet(root, 0, 0)?;
        reference.0.insert(type_id, (root, 0, 0));
        protected.insert(type_id);
    }
    let (mut creates, mut updates, mut deletes) = (0, 0, 0);
    for step in 0.. {
        if step >= 1600 && reference.0.len() >= 1000 {
            break;
        }
        let ids: Vec<u32> = reference.0.keys().copied().collect();
        let choice = if step >= 1600 { 0 } else { random.next(10) };
        if choice <= 6 {
            let type_id = ids[random.next(ids.len())];
            let from = part(&mut random, &ids);
            let to = part(&mut random, &ids);
            let index = triplets.create_triplet(type_id, from, to)?;
            assert!(reference.0.insert(index, (type_id, from, to)).is_none());
            creates += 1;
        } else if choice <= 8 {
            let index = ids[random.next(ids.len())];
            if protected.contains(&index) {
                continue;
            }
            let type_id = ids[random.next(ids.len())];
            let from = part(&mut random, &ids);
            let to = part(&mut random, &ids);
            triplets.update_triplet(index, type_id, from, to)?;
            reference.0.insert(index, (type_id, from, to));
            updates += 1;
        } else {
            let index = ids[random.next(ids.len())];
            if !protected.contains(&index) {
                let deleted = triplets.delete_triplet(index)?;
                let before = reference.0.len();
                reference.delete(index);
                assert_eq!(before - reference.0.len(), deleted.len());
                deletes += 1;
            }
        }
    }
    assert!(creates > 1000 && updates > 100 && deletes > 50);
    assert!(reference.0.len() >= 1000);

    assert_eq!(
        reference.each(TripletQuery::any()),
        triplets.each_triplet(TripletQuery::any())
    );
    let ids: Vec<u32> = reference.0.keys().copied().collect();
    for &id in ids.iter().chain([0].iter()) {
        for query in [
            TripletQuery::by_type(id),
            TripletQuery::by_from(id),
            TripletQuery::by_to(id),
            TripletQuery::by_index(id),
        ] {
            assert_eq!(
                reference.each(query),
                triplets.each_triplet(query),
                "{query:?}"
            );
            assert_eq!(reference.each(query).len(), triplets.count_triplets(query));
        }
    }
    for _ in 0..200 {
        let pick = |random: &mut Random| Some(part(random, &ids));
        let query = TripletQuery::new(
            None,
            pick(&mut random),
            pick(&mut random),
            pick(&mut random),
        );
        assert_eq!(
            reference.each(query),
            triplets.each_triplet(query),
            "{query:?}"
        );
        let (_, &(type_id, from, to)) = reference.0.iter().nth(random.next(ids.len())).unwrap();
        for query in [
            TripletQuery::new(None, Some(type_id), Some(from), None),
            TripletQuery::new(None, Some(type_id), None, Some(to)),
            TripletQuery::new(None, None, Some(from), Some(to)),
            TripletQuery::new(None, Some(type_id), Some(from), Some(to)),
        ] {
            assert_eq!(
                reference.each(query),
                triplets.each_triplet(query),
                "{query:?}"
            );
        }
    }
    assert_no_orphans(&mut triplets, false);

    while let Some((&index, _)) = reference.0.iter().next() {
        triplets.delete_triplet(index)?;
        reference.delete(index);
        assert_eq!(
            reference.0.len(),
            triplets.count_triplets(TripletQuery::any())
        );
    }
    let left = triplets.all_links();
    assert_eq!(2, left.len(), "only the markers are left: {left:?}");
    Ok(())
}

fn part(random: &mut Random, ids: &[u32]) -> u32 {
    if random.next(4) == 0 {
        0
    } else {
        ids[random.next(ids.len())]
    }
}

const PEOPLE: &str = "(Type: Type 0 0)
(Person: Type 0 0)
(knows: Type 0 0)
(alice: Person 0 0)
(bob: Person 0 0)
(carol: Person 0 0)
(r: knows alice bob)
";

fn named_people() -> Result<Store> {
    let mut triplets = store()?;
    import_triplets_lino(&mut triplets, PEOPLE)?;
    Ok(triplets)
}

fn id(triplets: &mut Store, name: &str) -> u32 {
    triplets.get_by_name(name).unwrap().unwrap()
}

fn named(triplets: &mut Store, names: [&str; 4]) -> Triplet {
    Triplet::new(
        id(triplets, names[0]),
        id(triplets, names[1]),
        if names[2] == "0" {
            0
        } else {
            id(triplets, names[2])
        },
        if names[3] == "0" {
            0
        } else {
            id(triplets, names[3])
        },
    )
}

#[test]
fn lino_round_trip_keeps_names_and_addresses() -> Result<()> {
    let mut triplets = named_people()?;
    assert_eq!(7, triplets.count_triplets(TripletQuery::any()));
    let r = named(&mut triplets, ["r", "knows", "alice", "bob"]);
    assert_eq!(Some(r), triplets.get_triplet(r.index));
    assert_eq!(PEOPLE, triplets_to_lino(&mut triplets)?);

    // Without names a triplet is its address, and the import keeps it.
    let mut numbered = store()?;
    let p = people(&mut numbered)?;
    numbered.create_triplet(p.knows, p.alice, p.bob)?;
    let gone = numbered.create_triplet(p.knows, p.alice, p.carol)?;
    numbered.create_triplet(p.knows, p.alice, p.bob)?;
    numbered.create_triplet(p.knows, 0, p.carol)?;
    numbered.delete_triplet(gone)?;
    let text = triplets_to_lino(&mut numbered)?;
    let mut copy = store()?;
    let imported = import_triplets_lino(&mut copy, &text)?;
    assert_eq!(numbered.count_triplets(TripletQuery::any()), imported.len());
    assert_eq!(
        numbered.each_triplet(TripletQuery::any()),
        copy.each_triplet(TripletQuery::any())
    );
    assert_eq!(text, triplets_to_lino(&mut copy)?);
    assert_no_orphans(&mut copy, false);

    // Lines may come in any order, and a line without an index is new.
    let mut forward = store()?;
    let imported = import_triplets_lino(
        &mut forward,
        "(r: knows alice bob)\n(knows: Type 0 0)\n(alice: Type 0 0)\n(bob: Type 0 0)\n(Type: Type 0 0)\n(knows bob alice)\n",
    )?;
    assert_eq!(6, imported.len());
    assert_eq!(
        Some(named(&mut forward, ["r", "knows", "alice", "bob"])),
        forward.get_triplet(imported[0])
    );
    let last = forward.get_triplet(imported[5]).unwrap();
    assert_eq!(
        (
            id(&mut forward, "knows"),
            id(&mut forward, "bob"),
            id(&mut forward, "alice")
        ),
        (last.type_id, last.from, last.to)
    );
    assert_no_orphans(&mut forward, false);
    Ok(())
}

#[test]
fn lino_import_refuses_what_is_not_a_triplet() -> Result<()> {
    let mut triplets = named_people()?;
    let markers = triplets.triplet_markers();
    let before = triplets_to_lino(&mut triplets)?;
    for text in [
        "(x: nobody 0 0)",
        "(x: Type 0)",
        "(x: $t 0 0)",
        "(x: Type * 0)",
        "(x: 0 0 0)",
        &format!("({}: Type 0 0)", markers.pair),
        &format!("(x: Type {} 0)", markers.nothing),
    ] {
        assert!(import_triplets_lino(&mut triplets, text).is_err(), "{text}");
    }
    assert_eq!(before, triplets_to_lino(&mut triplets)?);
    Ok(())
}

#[test]
fn quoted_triplet_references_remain_names_in_imports_and_queries() -> Result<()> {
    for (reference, name) in [
        ("'0'", "0"),
        ("\"1\"", "1"),
        ("`9999`", "9999"),
        ("'*'", "*"),
        ("'$item'", "$item"),
        ("~1{30}", "0"),
        ("'\"0\"'", "\"0\""),
        ("'x\\y'", "x\\y"),
        ("'a\nb'", "a\nb"),
        ("'#tag'", "#tag"),
        ("'~value'", "~value"),
        ("'  '", "  "),
        ("\"a'b`c\"", "a'b`c"),
        ("`a'b\"c`", "a'b\"c"),
    ] {
        let mut triplets = store()?;
        import_triplets_lino(
            &mut triplets,
            &format!("(Type: Type 0 0)\n({reference}: Type 0 0)\n(r: {reference} {reference} {reference})\n"),
        )?;
        let named_id = id(&mut triplets, name);
        let r = id(&mut triplets, "r");
        let before = Triplet::new(r, named_id, named_id, named_id);
        assert_eq!(Some(before), triplets.get_triplet(r), "{reference}");

        let text = triplets_to_lino(&mut triplets)?;
        let mut copy = store()?;
        import_triplets_lino(&mut copy, &text)?;
        assert_eq!(text, triplets_to_lino(&mut copy)?, "{reference}");
        assert!(copy.get_by_name(name)?.is_some(), "{reference}");

        let mut created = store()?;
        process_triplets_query(
            &mut created,
            &format!("() ((Type: Type 0 0) ({reference}: Type 0 0) (r: {reference} {reference} {reference}))"),
        )?;
        assert_eq!(text, triplets_to_lino(&mut created)?, "{reference}");
        assert_no_orphans(&mut created, false);

        let matches = link_cli::triplets::match_triplets_query(
            &mut triplets,
            &format!("(($r: {reference} {reference} {reference}))"),
            "$r",
        )?;
        assert_eq!(vec![r], matches, "{reference}");
        let changes = process_triplets_query(
            &mut triplets,
            &format!("((r: {reference} {reference} {reference})) ((r: {reference} 0 {reference}))"),
        )?;
        assert_eq!(
            vec![TripletChange {
                before: Some(before),
                after: Some(Triplet::new(r, named_id, 0, named_id)),
            }],
            changes,
            "{reference}"
        );
        // Quoted names used as substitution indices are looked up too.
        process_triplets_query(&mut triplets, &format!("() (({reference}: Type 0 0))"))?;
        assert_eq!(named_id, id(&mut triplets, name));
        assert_no_orphans(&mut triplets, false);
    }
    Ok(())
}

fn query(triplets: &mut Store, text: &str) -> Result<Vec<TripletChange>> {
    let changes = process_triplets_query(triplets, text)?;
    assert_no_orphans(triplets, false);
    Ok(changes)
}

#[test]
fn substitution_updates_the_to_part_in_place() -> Result<()> {
    let mut triplets = named_people()?;
    let before = named(&mut triplets, ["r", "knows", "alice", "bob"]);
    let after = named(&mut triplets, ["r", "knows", "alice", "carol"]);
    // The query of the issue: ((i: T f t)) ((i: T f t2)), by address.
    let text = format!(
        "((({}: {} {} {})) (({}: {} {} {})))",
        before.index,
        before.type_id,
        before.from,
        before.to,
        after.index,
        after.type_id,
        after.from,
        after.to
    );
    let changes = query(&mut triplets, &text)?;
    assert_eq!(
        vec![TripletChange {
            before: Some(before),
            after: Some(after)
        }],
        changes
    );
    assert_eq!(Some(after), triplets.get_triplet(before.index));
    // And by name, with a variable.
    let changes = query(
        &mut triplets,
        "((($r: knows alice carol)) (($r: knows alice bob)))",
    )?;
    assert_eq!(Some(before), changes[0].after);
    Ok(())
}

#[test]
fn substitution_reads_creates_and_deletes() -> Result<()> {
    let mut triplets = named_people()?;
    let r = named(&mut triplets, ["r", "knows", "alice", "bob"]);
    // A substitution that repeats the restriction only reads.
    let read = query(&mut triplets, "((($i: knows $f $t)) (($i: knows $f $t)))")?;
    assert_eq!(
        vec![TripletChange {
            before: Some(r),
            after: Some(r)
        }],
        read
    );
    let people_read = query(&mut triplets, "((($p: Person 0 0)) (($p: Person 0 0)))")?;
    assert_eq!(3, people_read.len());

    // A create for every match, with the variable of the match.
    let created = query(
        &mut triplets,
        "((($p: Person 0 0)) (($p: Person 0 0) (knows $p bob)))",
    )?;
    let new: Vec<Triplet> = created
        .iter()
        .filter(|change| change.before.is_none())
        .map(|change| change.after.unwrap())
        .collect();
    assert_eq!(3, new.len());
    let bob = id(&mut triplets, "bob");
    assert_eq!(4, triplets.count_triplets(TripletQuery::by_to(bob)));

    // No restriction: a create; a new name is a new triplet.
    let changes = query(
        &mut triplets,
        "(() ((likes: Type 0 0) (likes carol alice)))",
    )?;
    assert_eq!(2, changes.len());
    assert!(changes.iter().all(|change| change.before.is_none()));
    let likes = id(&mut triplets, "likes");
    assert_eq!(1, triplets.count_triplets(TripletQuery::by_type(likes)));

    // No substitution: a delete, with whatever used the deleted triplet.
    let deleted = query(&mut triplets, "((($i: knows $p bob)) ())")?;
    assert_eq!(4, deleted.len());
    assert!(deleted.iter().all(|change| change.after.is_none()));
    assert_eq!(0, triplets.count_triplets(TripletQuery::by_to(bob)));
    assert!(!triplets.is_triplet(r.index));
    Ok(())
}

#[test]
fn substitution_changes_the_type_part() -> Result<()> {
    let mut triplets = named_people()?;
    query(&mut triplets, "(() ((likes: Type 0 0)))")?;
    let changes = query(&mut triplets, "((($i: knows $f $t)) (($i: likes $f $t)))")?;
    let after = named(&mut triplets, ["r", "likes", "alice", "bob"]);
    assert_eq!(Some(after), changes[0].after);
    // `*` keeps a part, and the parts may swap.
    let changes = query(&mut triplets, "((($i: likes $f $t)) (($i: * $t $f)))")?;
    assert_eq!(
        Some(named(&mut triplets, ["r", "likes", "bob", "alice"])),
        changes[0].after
    );
    // A conjunction: who is liked by someone of type Person.
    let changes = query(
        &mut triplets,
        "((($i: likes $f $t) ($f: Person 0 0)) (($i: likes $f $t) ($f: Person 0 0)))",
    )?;
    assert_eq!(2, changes.len());
    Ok(())
}

#[test]
fn substitution_refuses_unknown_references() -> Result<()> {
    let mut triplets = named_people()?;
    let before = triplets_to_lino(&mut triplets)?;
    for text in [
        "(() ((nobody alice bob)))",
        "(() ((knows $x bob)))",
        "(() ((knows * bob)))",
        "(() ((*: knows alice bob)))",
        "(() ((knows alice)))",
        "(() ((0 alice bob)))",
        "((($i: knows $f $t)) (($i: knows $f $nobody)))",
    ] {
        assert!(
            process_triplets_query(&mut triplets, text).is_err(),
            "{text}"
        );
    }
    assert_eq!(before, triplets_to_lino(&mut triplets)?);
    // A restriction by an unknown name matches nothing.
    assert!(query(&mut triplets, "((($i: nobody $f $t)) ())")?.is_empty());
    Ok(())
}

/// The triplets are plain doublets, so a file-backed store keeps them across
/// a reopen and the named markers are found again.
#[test]
fn keeps_triplets_in_a_named_file_store() -> Result<()> {
    let file = NamedTempFile::new()?;
    let text = {
        let mut triplets = TripletsOverDoublets::with_named_markers(NamedTypesDecorator::new(
            file.path(),
            false,
        )?)?;
        import_triplets_lino(&mut triplets, PEOPLE)?;
        triplets.save()?;
        triplets_to_lino(&mut triplets)?
    };
    let mut triplets =
        TripletsOverDoublets::with_named_markers(NamedTypesDecorator::new(file.path(), false)?)?;
    assert_eq!(text, triplets_to_lino(&mut triplets)?);
    assert_eq!(PEOPLE, text);
    Ok(())
}
