use link_cli::engine::{
    ConcurrencyControl, Engine, EngineOptions, HistoryPolicy, IsolationLevel, MemoryLog,
};
use link_cli::LinksStorage;

fn engine(isolation: IsolationLevel) -> Engine<u64, MemoryLog> {
    Engine::open(
        MemoryLog::default(),
        EngineOptions {
            isolation,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn snapshots_hide_dirty_writes_and_rollback_discards_them() {
    let db = engine(IsolationLevel::Snapshot);
    let mut a = db.begin().unwrap();
    let b = db.begin().unwrap();
    let id = a.create_link(0, 0).unwrap();
    assert!(a.link_exists(id));
    assert!(!b.link_exists(id));
    a.rollback();
    assert!(db.snapshot().unwrap().links().is_empty());
}

#[test]
fn snapshot_conflicts_prevent_lost_updates() {
    let db = engine(IsolationLevel::Snapshot);
    let mut seed = db.begin().unwrap();
    let id = seed.create_link(0, 0).unwrap();
    seed.commit().unwrap();
    let mut a = db.begin().unwrap();
    let mut b = db.begin().unwrap();
    a.update_link(id, id, 0).unwrap();
    b.update_link(id, 0, id).unwrap();
    a.commit().unwrap();
    assert!(b.commit().is_err());
    assert_eq!(db.snapshot().unwrap().links()[0].source, id);
}

#[test]
fn serializable_validation_detects_write_skew_and_phantoms() {
    let db = engine(IsolationLevel::Serializable);
    let mut a = db.begin().unwrap();
    let mut b = db.begin().unwrap();
    assert_eq!(a.links_count(), 0);
    assert_eq!(b.links_count(), 0);
    a.create_link(0, 0).unwrap();
    b.create_link(0, 0).unwrap();
    a.commit().unwrap();
    assert!(b.commit().is_err());
}

#[test]
fn read_committed_refreshes_but_snapshot_is_repeatable() {
    for isolation in [IsolationLevel::ReadCommitted, IsolationLevel::Snapshot] {
        let db = engine(isolation);
        let observer = db.begin().unwrap();
        let mut writer = db.begin().unwrap();
        writer.create_link(0, 0).unwrap();
        writer.commit().unwrap();
        assert_eq!(
            observer.links_count(),
            usize::from(isolation == IsolationLevel::ReadCommitted)
        );
    }
}

#[test]
fn exclusive_control_releases_on_drop_and_history_is_bounded() {
    let db = Engine::<u64, _>::open(
        MemoryLog::default(),
        EngineOptions {
            concurrency: ConcurrencyControl::Exclusive,
            history: HistoryPolicy::Last(2),
            ..Default::default()
        },
    )
    .unwrap();
    let a = db.begin().unwrap();
    assert!(db.begin().is_err());
    drop(a);
    for _ in 0..3 {
        let mut tx = db.begin().unwrap();
        tx.create_link(0, 0).unwrap();
        tx.commit().unwrap();
    }
    assert!(db.snapshot_at(1).is_err());
    assert_eq!(db.snapshot_at(2).unwrap().links().len(), 2);
    assert_eq!(db.snapshot().unwrap().links().len(), 3);
}
