//! Tiny valid journals at the sequence boundary must reject writes without damage.
use link_cli::{
    CommitMode, DoubletsStorage, FileTransitionLog, GenericDoubletLink,
    GenericTransactionsDecorator, GenericTransition, LinksStorage, LogRetentionPolicy,
    TransitionKind, TransitionLogStore,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn verify(explicit: bool, cascade: bool) {
    let directory = tempfile::tempdir().unwrap();
    let data_path = directory.path().join("data");
    let log_path = directory.path().join("log");
    let open = || {
        GenericTransactionsDecorator::new(
            DoubletsStorage::<u32, _>::open(&data_path)
                .unwrap()
                .with_automatic_uniqueness_and_usages_resolution(),
            FileTransitionLog::open(&log_path).unwrap(),
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap()
    };
    let mut tx = open();
    let a = tx.create(0, 0).unwrap();
    let b = tx.create(a, a).unwrap();
    tx.create(b, b).unwrap();
    let baseline = tx.inner().all_links();
    tx.save().unwrap();
    drop(tx);
    let boundary = i64::MAX - i64::from(cascade);
    let state = GenericDoubletLink::new(a, 0, 0);
    let filler = GenericTransition {
        transaction_id: 1,
        sequence: boundary,
        timestamp_ms: 0,
        kind: TransitionKind::Update,
        before: state,
        after: state,
    };
    let mut log = FileTransitionLog::open(&log_path).unwrap();
    for entry in [
        format!("__transactions:transition:{}", filler.serialize()),
        format!("__transactions:commit:{:032x}", 1),
        format!("__transactions:applied:{boundary}"),
    ] {
        log.append_log_entry(&entry).unwrap();
    }
    log.flush_log().unwrap();
    drop(log);
    let mut tx = open();
    assert_eq!(tx.last_logged_sequence(), boundary);
    let log_count = tx.log().len();
    if explicit {
        tx.begin_transaction().unwrap();
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        if cascade {
            tx.delete(b).map(|_| ())
        } else {
            tx.create(a, b).map(|_| ())
        }
    }));
    assert!(
        result.is_ok(),
        "sequence exhaustion must return an error rather than panic"
    );
    assert!(
        result.unwrap().is_err(),
        "exhausted sequence space must reject the write"
    );
    if explicit {
        if cascade {
            assert!(tx.commit().is_err(), "incomplete undo batch cannot commit");
            assert!(
                tx.create(a, b).is_err(),
                "incomplete undo batch requires rollback"
            );
        }
        tx.rollback().unwrap();
    }
    assert_eq!(tx.inner().all_links(), baseline);
    assert_eq!(tx.last_logged_sequence(), boundary);
    assert_eq!(tx.log().len(), log_count);
    assert!(tx
        .log()
        .iter()
        .all(|record| record.sequence > 0 && record.sequence <= boundary));
    if cascade {
        // Failed two-row batch must not consume the one valid remaining number.
        tx.create(a, b).unwrap();
        assert_eq!(tx.last_logged_sequence(), i64::MAX);
    }
    let expected = tx.inner().all_links();
    tx.save().unwrap();
    drop(tx);
    let mut reopened = open();
    assert_eq!(reopened.inner().all_links(), expected);
    assert!(reopened.create(b, a).is_err());
    assert_eq!(reopened.inner().all_links(), expected);
    reopened.begin_transaction().unwrap();
    reopened.rollback().unwrap();
}
#[test]
fn exhausted_automatic_creation() {
    verify(false, false);
}
#[test]
fn exhausted_explicit_creation() {
    verify(true, false);
}
#[test]
fn exhausted_automatic_cascade_batch() {
    verify(false, true);
}
#[test]
fn exhausted_explicit_cascade_batch() {
    verify(true, true);
}
