//! Rollback must include upstream mapped-store cascade callbacks.
use link_cli::{
    CommitMode, DoubletsStorage, FileTransitionLog, GenericTransactionsDecorator, LinksStorage,
    LogRetentionPolicy,
};

fn verify(merge: bool) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let log = dir.path().join("log");
    let open = || {
        GenericTransactionsDecorator::new(
            DoubletsStorage::<u32, _>::open(&data)
                .unwrap()
                .with_automatic_uniqueness_and_usages_resolution(),
            FileTransitionLog::open(&log).unwrap(),
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap()
    };
    let mut tx = open();
    let a = tx.create(0, 0).unwrap();
    let b = tx.create(a, a).unwrap();
    let c = tx.create(b, b).unwrap();
    let baseline = tx.inner().all_links();
    tx.begin_transaction().unwrap();
    let mut changes = Vec::new();
    if merge {
        tx.update_observed(b, b, b, &mut |before, after| changes.push((before, after)))
            .unwrap();
    } else {
        tx.delete_observed(b, &mut |before, after| changes.push((before, after)))
            .unwrap();
    }
    assert!(
        changes.iter().any(|(before, _)| before.index == c),
        "dependent row must be observed"
    );
    tx.rollback().unwrap();
    assert_eq!(
        tx.inner().all_links(),
        baseline,
        "changes: {changes:?}; log: {:?}",
        tx.log()
    );
    drop(tx);
    assert_eq!(open().inner().all_links(), baseline);
}

#[test]
fn mapped_cascading_delete_rolls_back_every_row() {
    verify(false);
}

#[test]
fn mapped_uniqueness_merge_rolls_back_every_row() {
    verify(true);
}
