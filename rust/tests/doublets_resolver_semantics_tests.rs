//! Pins down the `doublets::decorators` semantics this crate relies on.
//!
//! The CLI delegates uniqueness and cascade resolution to the upstream
//! decorator stack instead of reimplementing it, so a change in upstream
//! behaviour is a change in `clink` behaviour. These tests exercise the
//! upstream stack directly — no `link-cli` types are involved — so that such a
//! change is reported here, against `doublets`, rather than as a puzzling
//! failure somewhere in the query processor.

use doublets::data::Flow;
use doublets::decorators::DecoratorsExt;
use doublets::mem::Global;
use doublets::unit::Store;
use doublets::Doublets;

fn dump(links: &impl Doublets<u32>) -> Vec<(u32, u32, u32)> {
    let mut all = Vec::new();
    links.each_links(&[], &mut |link| {
        all.push((link.index, link.source, link.target));
        Flow::Continue
    });
    all.sort_unstable();
    all
}

/// An update that turns a link into a duplicate of an existing one merges the
/// two, and every usage of the merged-away link is **rebased onto the
/// survivor** rather than blanked.
///
/// This is the behaviour the `update into duplicate` scenario in
/// `docs/case-studies/issue-100/evidence/cli-parity/run.sh` records as
/// diverging from C#: `Platform.Data.Doublets` 0.18.1 corrupts the usage
/// instead (see `../csharp-merge-usages` in the same folder). If this test
/// ever fails, doublets-rs has moved and the exemption needs revisiting.
#[test]
fn update_into_an_existing_pair_rebases_usages_onto_the_survivor() {
    let mut links = Store::<u32, _>::new(Global::new())
        .unwrap()
        .with_automatic_uniqueness_and_usages_resolution();

    let one = links.create_link(1, 2).unwrap();
    // `two` uses `one` as its target, so the merge below has something to
    // rebase.
    let two = links.create_link(2, one).unwrap();
    assert_eq!(dump(&links), vec![(1, 1, 2), (2, 2, 1)]);

    // Updating `one` to `(2, 1)` makes it a duplicate of `two`.
    links.update(one, 2, 1).unwrap();

    // `one` is gone, and `two`'s dangling target now points at `two` itself
    // — the address the merge kept — instead of at a deleted link.
    assert_eq!(dump(&links), vec![(two, 2, two)]);
}

/// Deleting a link deletes everything that referenced it, transitively.
#[test]
fn deleting_a_link_cascades_to_its_usages() {
    let mut links = Store::<u32, _>::new(Global::new())
        .unwrap()
        .with_automatic_uniqueness_and_usages_resolution();

    let one = links.create_link(1, 1).unwrap();
    let two = links.create_link(2, 2).unwrap();
    let usage = links.create_link(one, two).unwrap();
    assert_eq!(dump(&links), vec![(1, 1, 1), (2, 2, 2), (usage, one, two)]);

    links.delete(two).unwrap();

    assert_eq!(dump(&links), vec![(one, 1, 1)]);
}
