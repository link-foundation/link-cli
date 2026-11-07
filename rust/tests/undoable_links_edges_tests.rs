//! The edges of the undo log and of the store that fills it.
#![cfg(not(target_arch = "wasm32"))]
use link_cli::undoable_links::{keep_names_before_update, usages_of};
use link_cli::{Link, LinksUndoLog, NamedTypeLinks, NamedTypesDecorator, UndoableLinksDecorator};
#[path = "common/storage.rs"]
mod test_store;

fn named() -> (test_store::NamedFiles, NamedTypesDecorator) {
    test_store::named().unwrap()
}

#[test]
fn an_empty_log_is_empty_until_it_keeps_a_change_or_a_name() {
    let (_owner, mut links) = named();
    let id = links.create(0, 0);
    let mut log = LinksUndoLog::new();
    assert!(log.is_empty());
    log.keep_name(&mut links, id).unwrap();
    assert!(!log.is_empty());
    log.clear();
    assert!(log.is_empty());
    log.record(Some(Link::new(1, 2, 3)), Some(Link::new(1, 2, 3)));
    assert!(log.is_empty(), "a change that changes nothing is not kept");
    log.record(None, None);
    assert!(!log.is_empty());
    assert_eq!(log.changes(), [(None, None)]);
    assert!(log.undo(&mut links).unwrap().is_empty());
    assert!(log.is_empty());
}

#[test]
fn the_null_link_reported_by_an_observer_is_no_link() {
    let mut log = LinksUndoLog::new();
    log.record_observed(Link::new(0, 0, 0), Link::new(4, 1, 1));
    log.record_observed(Link::new(4, 1, 1), Link::new(0, 0, 0));
    assert_eq!(
        log.changes(),
        [
            (None, Some(Link::new(4, 1, 1))),
            (Some(Link::new(4, 1, 1)), None)
        ]
    );
}

#[test]
fn undo_tolerates_links_that_are_already_gone_and_restores_empty_links() {
    let (_owner, mut links) = named();
    let mut log = LinksUndoLog::new();
    // Created, then already gone before the undo.
    log.record(None, Some(Link::new(40, 1, 1)));
    // Updated, then already gone before the undo.
    log.record(Some(Link::new(41, 1, 1)), Some(Link::new(41, 2, 2)));
    // Deleted empty link, gone: it comes back without an update.
    log.record(Some(Link::new(42, 0, 0)), None);
    assert_eq!(log.undo(&mut links).unwrap(), [40]);
    assert!(!links.exists(40));
    assert!(!links.exists(41));
    let back = links.get_link(42).unwrap();
    assert_eq!((back.source, back.target), (0, 0));

    // A deleted link that exists again gets its parts back, at the same address.
    let again = links.create(7, 7);
    let mut log = LinksUndoLog::new();
    log.record(Some(Link::new(again, 8, 9)), None);
    log.undo(&mut links).unwrap();
    let back = links.get_link(again).unwrap();
    assert_eq!((back.source, back.target), (8, 9));
}

#[test]
fn undo_takes_names_given_by_writes_and_gives_back_the_old_ones() {
    let (_owner, mut links) = named();
    let (plain, called, moved) = (links.create(0, 0), links.create(0, 0), links.create(0, 0));
    links.set_name(called, "old").unwrap();
    links.set_name(moved, "kept").unwrap();
    let mut log = LinksUndoLog::new();
    log.keep_name(&mut links, plain).unwrap();
    log.keep_name(&mut links, called).unwrap();
    log.keep_name(&mut links, moved).unwrap();
    log.keep_name(&mut links, moved).unwrap();
    links.set_name(plain, "given").unwrap();
    links.set_name(called, "new").unwrap();
    log.undo(&mut links).unwrap();
    assert_eq!(links.get_name(plain).unwrap(), None);
    assert_eq!(links.get_name(called).unwrap().as_deref(), Some("old"));
    assert_eq!(links.get_name(moved).unwrap().as_deref(), Some("kept"));
    assert_eq!(links.get_by_name("given").unwrap(), None);
}

#[test]
fn undo_drops_the_name_of_a_link_that_is_gone() {
    let (_owner, mut links) = named();
    let id = links.create(0, 0);
    links.set_name(id, "gone").unwrap();
    let mut log = LinksUndoLog::new();
    log.keep_name(&mut links, id).unwrap();
    links.remove_name(id).unwrap();
    links.delete(id).unwrap();
    log.undo(&mut links).unwrap();
    assert!(!links.exists(id));
    assert_eq!(links.get_name(id).unwrap(), None);
}

#[test]
fn a_name_holder_is_kept_only_when_someone_holds_it() {
    let (_owner, mut links) = named();
    let id = links.create(0, 0);
    let mut log = LinksUndoLog::new();
    log.keep_holder_of(&mut links, "nobody").unwrap();
    assert!(log.is_empty());
    links.set_name(id, "held").unwrap();
    log.keep_holder_of(&mut links, "held").unwrap();
    assert!(!log.is_empty());
}

#[test]
fn usages_reach_through_parts_and_stop_at_cycles() {
    let (_owner, mut links) = named();
    let base = links.create(0, 0);
    let middle = links.create(base, 0);
    let top = links.create(0, middle);
    links.update(base, top, 0).unwrap();
    let mut usages = usages_of(&mut links, base);
    assert_eq!(usages.remove(0), base);
    usages.sort_unstable();
    assert_eq!(usages, [middle, top]);
}

#[test]
fn an_update_that_merges_keeps_the_names_of_the_usages() {
    let (_owner, mut links) = named();
    let equal = links.create(5, 6);
    let moved = links.create(5, 7);
    let usage = links.create(moved, 0);
    links.set_name(usage, "usage").unwrap();

    let mut plain = LinksUndoLog::new();
    keep_names_before_update(&mut plain, &mut links, moved, 8, 9).unwrap();
    links.remove_name(usage).unwrap();
    plain.undo(&mut links).unwrap();
    assert_eq!(links.get_name(usage).unwrap(), None, "usage was not kept");

    links.set_name(usage, "usage").unwrap();
    let mut merging = LinksUndoLog::new();
    keep_names_before_update(&mut merging, &mut links, moved, 5, 6).unwrap();
    links.remove_name(usage).unwrap();
    merging.undo(&mut links).unwrap();
    assert_eq!(links.get_name(usage).unwrap().as_deref(), Some("usage"));
    assert!(links.exists(equal));

    // An update onto itself is not a merge.
    let mut itself = LinksUndoLog::new();
    keep_names_before_update(&mut itself, &mut links, equal, 5, 6).unwrap();
}

#[test]
fn the_decorator_forwards_what_it_does_not_keep() {
    let (_owner, links) = named();
    let mut undo = UndoableLinksDecorator::new(links);
    assert!(!undo.recording());
    assert!(undo.query_limits().is_none());
    let id = undo.create(0, 0);
    assert!(undo.reference_exists(id));
    assert_eq!(undo.get_link(id).unwrap().index, id);
    assert_eq!(undo.next_created_addresses(1).unwrap(), [id + 1]);
    assert_eq!(undo.get_by_name("nobody").unwrap(), None);
    assert!(undo.log().is_empty());
    assert!(undo.inner_mut().exists(id));
    assert!(undo.into_inner().exists(id));
}

#[test]
fn writes_outside_a_request_stay_and_are_not_kept() {
    let (_owner, links) = named();
    let mut undo = UndoableLinksDecorator::new(links);
    let id = undo.create(0, 0);
    undo.update(id, 1, 1).unwrap();
    undo.set_name(id, "free").unwrap();
    undo.remove_name(id).unwrap();
    undo.delete(id).unwrap();
    assert!(undo.log().is_empty());
    assert!(undo.rollback().unwrap().is_empty());
    assert!(!undo.exists(id));
}

#[test]
fn every_way_of_creating_is_kept_and_undone() {
    let (_owner, links) = named();
    let mut undo = UndoableLinksDecorator::new(links);
    let existing = undo.create(1, 1);
    undo.begin();
    assert!(undo.recording());
    assert_eq!(undo.ensure_created(0), 0);
    assert_eq!(undo.ensure_created(existing), existing);
    assert_eq!(undo.try_ensure_created(existing).unwrap(), existing);
    assert!(undo.log().is_empty(), "nothing was created yet");
    assert!(undo.try_ensure_created(0).is_err());
    let ensured = undo.ensure_created(10);
    let tried = undo.try_ensure_created(11).unwrap();
    let made = undo.try_create(2, 2).unwrap();
    let got = undo.get_or_create(3, 3);
    assert_eq!(undo.get_or_create(3, 3), got);
    let created = undo.rollback().unwrap();
    assert!(!undo.recording());
    for id in [ensured, tried, made, got] {
        assert!(created.contains(&id), "{id} in {created:?}");
        assert!(!undo.exists(id));
    }
    assert!(undo.exists(existing));
}

#[test]
fn commit_keeps_the_writes_and_forgets_how_to_undo_them() {
    let (_owner, links) = named();
    let mut undo = UndoableLinksDecorator::new(links);
    undo.begin();
    let id = undo.create(1, 1);
    undo.set_name(id, "stays").unwrap();
    undo.commit();
    assert!(undo.log().is_empty());
    assert!(undo.rollback().unwrap().is_empty());
    assert_eq!(undo.get_name(id).unwrap().as_deref(), Some("stays"));
    undo.save().unwrap();
}

#[test]
fn undoing_a_request_restores_an_update_a_delete_and_a_stolen_name() {
    let (_owner, links) = named();
    let mut undo = UndoableLinksDecorator::new(links);
    let (kept, doomed, holder) = (undo.create(1, 1), undo.create(2, 2), undo.create(3, 3));
    undo.set_name(doomed, "doomed").unwrap();
    undo.set_name(holder, "shared").unwrap();
    undo.begin();
    undo.update(kept, 9, 9).unwrap();
    undo.delete(doomed).unwrap();
    undo.remove_name(holder).unwrap();
    undo.set_name(kept, "shared").unwrap();
    undo.rollback().unwrap();
    let row = undo.get_link(kept).unwrap();
    assert_eq!((row.source, row.target), (1, 1));
    assert_eq!(undo.get_name(doomed).unwrap().as_deref(), Some("doomed"));
    assert_eq!(undo.get_name(kept).unwrap(), None);
    assert_eq!(undo.get_by_name("shared").unwrap(), Some(holder));
}
