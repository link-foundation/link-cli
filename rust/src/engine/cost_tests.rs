//! Cost assertions observe private bookkeeping without expanding the public API.
use super::*;
use crate::LinksStorage;

fn count(db: &Engine<u64, MemoryLog>) -> usize {
    db.state.lock().unwrap().changed.len()
}
fn seed(options: EngineOptions) -> Engine<u64, MemoryLog> {
    let db = Engine::open(MemoryLog::default(), options).unwrap();
    let mut tx = db.begin().unwrap();
    for id in 1..=128 {
        tx.ensure_link_created(id).unwrap();
    }
    tx.commit().unwrap();
    db
}

#[test]
fn replay_and_nonoverlapping_sessions_need_no_generations() {
    for concurrency in [
        ConcurrencyControl::Optimistic,
        ConcurrencyControl::Exclusive,
    ] {
        for isolation in [
            IsolationLevel::Serializable,
            IsolationLevel::Snapshot,
            IsolationLevel::ReadCommitted,
        ] {
            let options = EngineOptions {
                concurrency,
                isolation,
                ..Default::default()
            };
            let db = seed(options.clone());
            assert_eq!(count(&db), 0, "single writer {concurrency:?}/{isolation:?}");
            let log = db.with_log(|log| Ok(log.clone())).unwrap();
            let reopened = Engine::open(log, options).unwrap();
            assert_eq!(count(&reopened), 0, "replay {concurrency:?}/{isolation:?}");
            let mut tx = reopened.begin().unwrap();
            tx.update_link(1, 2, 3).unwrap();
            tx.commit().unwrap();
            assert_eq!(reopened.snapshot().unwrap().get(1).unwrap().link.source, 2);
            assert_eq!(count(&reopened), 0);
        }
    }
}

#[test]
fn serializable_validation_needs_no_row_generations() {
    let db = seed(EngineOptions::default());
    let mut stale = db.begin().unwrap();
    let mut writer = db.begin().unwrap();
    writer.update_link(1, 2, 3).unwrap();
    writer.commit().unwrap();
    assert_eq!(count(&db), 0);
    stale.update_link(2, 3, 4).unwrap();
    assert!(stale
        .commit()
        .unwrap_err()
        .to_string()
        .contains("serializable"));
    assert_eq!(count(&db), 0);
}

#[test]
fn optimistic_generations_cover_aba_and_disjoint_writes_then_are_reclaimed() {
    for isolation in [IsolationLevel::ReadCommitted, IsolationLevel::Snapshot] {
        let db = seed(EngineOptions {
            isolation,
            ..Default::default()
        });
        let mut stale = db.begin().unwrap();
        stale.update_link(1, 1, 0).unwrap();
        let mut deleted = db.begin().unwrap();
        deleted.delete_link(1).unwrap();
        deleted.commit().unwrap();
        assert_eq!(count(&db), 1);
        let mut recreated = db.begin().unwrap();
        recreated.ensure_link_created(1).unwrap();
        recreated.commit().unwrap();
        assert_eq!(count(&db), 1);
        assert!(stale.commit().unwrap_err().to_string().contains("conflict"));
        assert_eq!(count(&db), 0, "last conflicting committer");
        assert_eq!(db.snapshot().unwrap().get(1).unwrap().link.source, 0);

        let mut first = db.begin().unwrap();
        let mut second = db.begin().unwrap();
        first.update_link(1, 1, 0).unwrap();
        second.update_link(2, 2, 0).unwrap();
        first.commit().unwrap();
        assert_eq!(count(&db), 1);
        second.commit().unwrap();
        assert_eq!(count(&db), 0);
        assert_eq!(db.snapshot().unwrap().get(2).unwrap().link.source, 2);

        for finish in 0..3 {
            let mut observer = db.begin().unwrap();
            let mut writer = db.begin().unwrap();
            writer.update_link(3, 3, 0).unwrap();
            writer.commit().unwrap();
            assert_eq!(count(&db), 1);
            match finish {
                0 => {
                    observer.commit().unwrap();
                }
                1 => observer.rollback(),
                _ => drop(observer),
            }
            assert_eq!(count(&db), 0, "read-only commit/rollback/drop {finish}");
        }
    }
}

#[test]
fn branch_publication_tracks_only_changes_that_can_conflict_with_active_sessions() {
    let db = seed(EngineOptions {
        isolation: IsolationLevel::Snapshot,
        ..Default::default()
    });
    let mut branch = db
        .fork(MemoryLog::default(), EngineOptions::default())
        .unwrap();
    let mut stale = db.begin().unwrap();
    stale.update_link(1, 1, 0).unwrap();
    let mut incoming = branch.engine().begin().unwrap();
    incoming.update_link(1, 2, 0).unwrap();
    incoming.commit().unwrap();
    branch.merge().unwrap();
    assert_eq!(count(&db), 1);
    assert!(stale.commit().is_err());
    assert_eq!(count(&db), 0);
    assert_eq!(db.snapshot().unwrap().get(1).unwrap().link.source, 2);
}

#[test]
fn point_read_sets_are_selected_and_reclaimed_with_generations() {
    for concurrency in [
        ConcurrencyControl::Optimistic,
        ConcurrencyControl::Exclusive,
    ] {
        for validation in [
            SerializableValidation::Revision,
            SerializableValidation::ReadSet,
        ] {
            let selected = concurrency == ConcurrencyControl::Optimistic
                && validation == SerializableValidation::ReadSet;
            let db = seed(EngineOptions {
                concurrency,
                serializable_validation: validation,
                ..Default::default()
            });
            let mut reader = db.begin().unwrap();
            assert_eq!(reader.reads.is_some(), selected);
            reader.get_link(1);
            if selected {
                assert_eq!(reader.reads.as_ref().unwrap().lock().unwrap().keys.len(), 1);
                let mut writer = db.begin().unwrap();
                writer.update_link(2, 2, 0).unwrap();
                writer.commit().unwrap();
                assert_eq!(count(&db), 1);
            }
            reader.commit().unwrap();
            assert!(reader.reads.is_none());
            assert_eq!(count(&db), 0);
            // Closed reads preserve the prior API without retaining dependencies.
            reader.get_link(1);
            reader.all_links();
            assert!(reader.reads.is_none());
            let mut discarded = db.begin().unwrap();
            discarded.get_link(1);
            discarded.rollback();
            assert!(discarded.reads.is_none());
        }
    }
}

#[test]
fn a_poisoned_read_set_closes_and_releases_the_session_without_publishing() {
    let db = seed(EngineOptions {
        serializable_validation: SerializableValidation::ReadSet,
        ..Default::default()
    });
    let mut tx = db.begin().unwrap();
    tx.update_link(1, 2, 3).unwrap();
    std::thread::scope(|scope| {
        assert!(scope
            .spawn(|| {
                let _guard = tx.reads.as_ref().unwrap().lock().unwrap();
                panic!("injected failure while recording a read");
            })
            .join()
            .is_err());
    });
    assert!(tx
        .commit()
        .unwrap_err()
        .to_string()
        .contains("read-set mutex poisoned"));
    assert_eq!(db.snapshot().unwrap().get(1).unwrap().link.source, 0);
    assert_eq!(db.state.lock().unwrap().active, 0);
    assert_eq!(count(&db), 0);
    let mut fresh = db.begin().unwrap();
    fresh.update_link(1, 3, 2).unwrap();
    fresh.commit().unwrap();
}
