//! Finite fault injection for boundaries unavailable through normal-sized input.
use super::*;
use crate::LinksStorage;

#[test]
fn revision_exhaustion_closes_without_appending_or_poisoning_the_engine() {
    let db = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    // A bounded fixture, not 2^64 commits. Publication must check before append.
    db.state.lock().unwrap().current.revision = u64::MAX;
    let mut tx = db.begin().unwrap();
    tx.create_link(0, 0).unwrap();
    assert!(tx
        .commit()
        .unwrap_err()
        .to_string()
        .contains("revision overflow"));
    assert!(tx.commit().is_err());
    let state = db.state.lock().unwrap();
    assert_eq!(state.active, 0);
    assert!(!state.poisoned);
    assert!(state.log.entries.is_empty());
    assert!(state.current.rows.is_empty());
    drop(state);
    // Exhaustion blocks new writes, not inspection or read-only sessions.
    assert_eq!(db.snapshot_at(u64::MAX).unwrap().revision, u64::MAX);
    assert_eq!(db.begin().unwrap().commit().unwrap(), u64::MAX);
}

#[test]
fn mutex_poisoning_refuses_sessions_but_preserves_the_recoverable_log() {
    let db = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = db.begin().unwrap();
    tx.create_link(0, 0).unwrap();
    tx.commit().unwrap();
    drop(tx);
    std::thread::scope(|scope| {
        assert!(scope
            .spawn(|| {
                let _guard = db.state.lock().unwrap();
                panic!("injected owner failure");
            })
            .join()
            .is_err());
    });
    assert!(db
        .snapshot()
        .unwrap_err()
        .to_string()
        .contains("mutex poisoned"));
    assert!(db.begin().is_err());
    let log = db.into_log().ok().unwrap();
    let reopened = Engine::<u64, _>::open(log, EngineOptions::default()).unwrap();
    assert_eq!(
        reopened.snapshot().unwrap().links(),
        [GenericLink::new(1, 0, 0)]
    );
}

#[test]
fn dropping_a_session_after_mutex_poisoning_releases_admission() {
    let db = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = db.begin().unwrap();
    tx.create_link(0, 0).unwrap();
    std::thread::scope(|scope| {
        assert!(scope
            .spawn(|| {
                let _guard = db.state.lock().unwrap();
                panic!("injected failure with an active session");
            })
            .join()
            .is_err());
    });
    drop(tx);
    let state = db.state.lock().err().unwrap().into_inner();
    assert_eq!(state.active, 0);
    assert!(state.current.rows.is_empty());
    assert!(state.log.entries.is_empty());
}

#[test]
fn trace_selection_does_not_change_publication_or_conflict_results() {
    for concurrency in [
        ConcurrencyControl::Exclusive,
        ConcurrencyControl::Optimistic,
    ] {
        for isolation in [IsolationLevel::Serializable, IsolationLevel::Snapshot] {
            for trace in [false, true] {
                let db = Engine::<u64, _>::open(
                    MemoryLog::default(),
                    EngineOptions {
                        concurrency,
                        isolation,
                        trace,
                        ..Default::default()
                    },
                )
                .unwrap();
                let mut tx = db.begin().unwrap();
                tx.ensure_link_created(1).unwrap();
                tx.commit().unwrap();
                let overlapping =
                    (concurrency == ConcurrencyControl::Optimistic).then(|| db.begin().unwrap());
                let mut tx = db.begin().unwrap();
                tx.update_link(1, 1, 0).unwrap();
                tx.commit().unwrap();
                assert_eq!(db.snapshot().unwrap().get(1).unwrap().link.source, 1);
                assert_eq!(
                    db.state.lock().unwrap().changed.is_empty(),
                    concurrency == ConcurrencyControl::Exclusive
                        || isolation == IsolationLevel::Serializable
                );
                drop(overlapping);
                assert!(db.state.lock().unwrap().changed.is_empty());
            }
        }
    }
}
