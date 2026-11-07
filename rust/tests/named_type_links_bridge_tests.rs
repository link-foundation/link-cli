//! Every layer answers the same `NamedTypeLinks` calls, also through a lent `&mut`.
#![cfg(not(target_arch = "wasm32"))]
use link_cli::{
    CommitMode, LogRetentionPolicy, NamedTypeLinks, TransactionsDecorator, VersionControlDecorator,
};
#[path = "common/storage.rs"]
mod test_store;

/// Written against the trait alone, so each implementation runs the same calls.
fn bridge<L: NamedTypeLinks>(links: &mut L) {
    assert_eq!(links.create(0, 0), 1);
    assert_eq!(links.next_created_addresses(2).unwrap(), [2, 3]);
    assert_eq!(links.get_or_create(5, 6), 2);
    assert_eq!(links.get_or_create(5, 6), 2);
    assert_eq!(links.search(5, 6), Some(2));
    assert_eq!(links.ensure_created(9), 9);
    assert_eq!(links.try_ensure_created(9).unwrap(), 9);
    assert_eq!(links.try_ensure_created(12).unwrap(), 12);
    assert!(links.try_ensure_created(0).is_err());
    assert!(links.exists(12));
    assert_eq!(links.get_link(2).map(|link| link.target), Some(6));
}

fn transactions() -> (Vec<test_store::NamedFiles>, TransactionsDecorator) {
    let (data_owner, data) = test_store::named().unwrap();
    let (log_owner, log) = test_store::named().unwrap();
    let tx = TransactionsDecorator::new(
        data,
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap();
    (vec![data_owner, log_owner], tx)
}

#[test]
fn the_plain_storage_answers_the_bridge_also_when_lent() {
    let (_owner, mut links) = test_store::links().unwrap();
    bridge(&mut links);
    let (_owner, mut lent) = test_store::links().unwrap();
    bridge(&mut &mut lent);
}

#[test]
fn the_named_storage_answers_the_bridge() {
    let (_owner, mut links) = test_store::named().unwrap();
    bridge(&mut links);
}

#[test]
fn the_transaction_layer_answers_the_bridge_also_when_lent() {
    let (_owners, mut links) = transactions();
    bridge(&mut links);
    let (_owners, mut lent) = transactions();
    bridge(&mut &mut lent);
}

#[test]
fn the_version_control_layer_answers_the_bridge() {
    let (_owners, tx) = transactions();
    let (_branches_owner, branches) = test_store::named().unwrap();
    let mut links = VersionControlDecorator::new(tx, branches, false).unwrap();
    bridge(&mut links);
}
