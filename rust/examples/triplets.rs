//! Triplets `(index: type from to)` stored in doublets (issue #151).
//!
//! Run it with:
//!
//! ```text
//! cargo run --manifest-path rust/Cargo.toml --example triplets
//! ```
//!
//! It imports a few people as Links Notation, changes a triplet by a
//! substitution query and by the API, deletes one, prints the triplets and
//! the doublets that hold them, and then stores 1011 triplets made by
//! the generator of the parity test to count the doublets they take
//! (`experiments/issue-151/encodings.py` predicts 2317 plus the two markers).

use anyhow::Result;
use link_cli::{
    import_triplets_lino, process_triplets_query, triplets_to_lino, LinkStorage, NamedTypeLinks,
    TripletQuery, Triplets, TripletsOverDoublets,
};

const PEOPLE: &str = "(Type: Type 0 0)
(Person: Type 0 0)
(knows: Type 0 0)
(alice: Person 0 0)
(bob: Person 0 0)
(carol: Person 0 0)
(r: knows alice bob)
";

/// The LCG of the tests, so the triplets are made as in the parity test.
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

fn main() -> Result<()> {
    // The points named Pair and Nothing are created when missing.
    let mut triplets = TripletsOverDoublets::with_named_markers(
        LinkStorage::in_memory(false).with_indexed_lookups(),
    )?;
    import_triplets_lino(&mut triplets, PEOPLE)?;

    // ((i: T f t)) ((i: T f t2)): the address of r stays.
    for change in process_triplets_query(
        &mut triplets,
        "((($r: knows alice bob)) (($r: knows alice carol)))",
    )? {
        println!("{:?} -> {:?}", change.before, change.after);
    }
    let knows = triplets.get_by_name("knows")?.unwrap_or_default();
    let bob = triplets.get_by_name("bob")?.unwrap_or_default();
    let carol = triplets.get_by_name("carol")?.unwrap_or_default();
    let back = triplets.create_triplet(knows, carol, bob)?;
    println!(
        "by to carol: {:?}",
        triplets.each_triplet(TripletQuery::by_to(carol))
    );
    println!(
        "by type knows: {}",
        triplets.count_triplets(TripletQuery::by_type(knows))
    );
    triplets.delete_triplet(back)?;

    println!();
    print!("{}", triplets_to_lino(&mut triplets)?);
    println!();
    for link in triplets.all_links() {
        println!(
            "({}: {} {}) {:?}",
            link.index,
            link.source,
            link.target,
            triplets.role_of(link.index)
        );
    }

    // Creates only, made as in the parity test: a root, 10 types, then 1000 triplets.
    let mut triplets = TripletsOverDoublets::with_named_markers(
        LinkStorage::in_memory(false).with_indexed_lookups(),
    )?;
    let mut random = Random(151);
    let root = triplets.create_triplet_point()?;
    let mut ids = vec![root];
    for _ in 0..10 {
        ids.push(triplets.create_triplet(root, 0, 0)?);
    }
    while ids.len() < 1011 {
        let type_id = ids[random.next(ids.len())];
        let mut part = |ids: &[u32]| {
            if random.next(4) == 0 {
                0
            } else {
                ids[random.next(ids.len())]
            }
        };
        let from = part(&ids);
        let to = part(&ids);
        ids.push(triplets.create_triplet(type_id, from, to)?);
    }
    let doublets = triplets.all_links().len();
    println!();
    println!(
        "{} triplets in {doublets} doublets ({} besides the two markers)",
        triplets.count_triplets(TripletQuery::any()),
        doublets - 2
    );
    Ok(())
}
