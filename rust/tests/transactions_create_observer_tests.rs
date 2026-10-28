//! Creation may merge into an existing row or fail after allocating a temporary.
use link_cli::{
    decorators::{DecoratorsExt, Validate},
    CommitMode, DoubletsStorage, FileTransitionLog, GenericTransactionsDecorator, LinksStorage,
    LogRetentionPolicy,
};

fn verify_resolved(composite: bool, recover: bool) {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let log = directory.path().join("log");
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
    if composite {
        let created = tx.create_and_update(a, c).unwrap();
        assert_ne!(created, a, "composite create must allocate a fresh row");
        assert_eq!(tx.inner().get_link(a).unwrap(), baseline[0]);
        assert_eq!(tx.inner().get_link(created).unwrap().source, a);
    } else {
        assert_eq!(tx.create(a, a).unwrap(), b);
        assert_eq!(tx.inner().all_links(), baseline);
    }
    if recover {
        tx.save().unwrap();
    } else {
        tx.rollback().unwrap();
        assert_eq!(tx.inner().all_links(), baseline);
    }
    drop(tx);
    assert_eq!(open().inner().all_links(), baseline);
}
#[test]
fn duplicate_creation_preserves_existing_rows_on_rollback() {
    verify_resolved(false, false);
}
#[test]
fn duplicate_creation_preserves_existing_rows_on_recovery() {
    verify_resolved(false, true);
}
#[test]
fn composite_creation_preserves_existing_empty_row_on_rollback() {
    verify_resolved(true, false);
}
#[test]
fn composite_creation_preserves_existing_empty_row_on_recovery() {
    verify_resolved(true, true);
}

fn verify_validation(explicit: bool) {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let log = directory.path().join("log");
    let open = || {
        GenericTransactionsDecorator::new(
            DoubletsStorage::<u32, _>::open(&data)
                .unwrap()
                .map_store(|store| store.with_uniqueness(Validate)),
            FileTransitionLog::open(&log).unwrap(),
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap()
    };
    let mut tx = open();
    let a = tx.create(11, 12).unwrap();
    tx.create(a, a).unwrap();
    let baseline = tx.inner().all_links();
    if explicit {
        tx.begin_transaction().unwrap();
    }
    assert!(tx.create(a, a).is_err());
    if explicit {
        tx.rollback().unwrap();
    }
    assert_eq!(
        tx.inner().all_links(),
        baseline,
        "failed create must undo its temporary row"
    );
    tx.begin_transaction().unwrap();
    tx.rollback().unwrap();
    drop(tx);
    assert_eq!(open().inner().all_links(), baseline);
}
#[test]
fn rejected_automatic_creation_undoes_temporary_row() {
    verify_validation(false);
}
#[test]
fn rejected_explicit_creation_keeps_temporary_row_reversible() {
    verify_validation(true);
}
