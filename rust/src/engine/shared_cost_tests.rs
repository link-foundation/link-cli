//! Costs and immutable boundaries which are invisible to the links API.
use super::*;
use crate::LinksStorage;

fn shared() -> EngineOptions {
    EngineOptions {
        state_structure: StateStructure::Shared,
        history: HistoryPolicy::All,
        ..Default::default()
    }
}
fn unmaterialized(snapshot: &Snapshot<u64>) {
    match snapshot.rows.as_ref() {
        Rows::Shared { compatibility, .. } => assert!(compatibility.get().is_none()),
        Rows::Flat(_) => panic!("selected shared representation lost"),
    }
}

#[test]
fn internal_reads_commits_recovery_and_branches_do_not_materialize_compatibility_maps() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Engine<u64, MemoryLog>>();
    send_sync::<Snapshot<u64>>();
    let db = Engine::<u64, _>::open(MemoryLog::default(), shared()).unwrap();
    let mut seed = db.begin().unwrap();
    for id in 1..=64 {
        seed.ensure_link_created(id).unwrap();
        seed.set_name(id, Some(format!("row {id}"))).unwrap();
    }
    seed.commit().unwrap();
    let original = db.snapshot().unwrap();
    unmaterialized(&original);
    let mut reader = db.begin().unwrap();
    assert_eq!(reader.get_by_name("row 64"), Some(64));
    assert_eq!(reader.all_links().len(), 64);
    reader.commit().unwrap();
    assert!(Arc::ptr_eq(&original.rows, &db.snapshot().unwrap().rows));
    unmaterialized(&original);
    // Explicitly request the old public map; later candidates must neither
    // copy nor share its mutable initialization cell.
    let old_map = original.rows();
    assert_eq!(old_map.len(), 64);
    let cloned = original.clone();
    assert!(std::ptr::eq(cloned.rows(), old_map));
    let mut changed = db.begin().unwrap();
    changed.update_link(1, 2, 3).unwrap();
    changed.commit().unwrap();
    let current = db.snapshot().unwrap();
    unmaterialized(&current);
    assert!(std::ptr::eq(
        original.get(64).unwrap(),
        current.get(64).unwrap()
    ));
    let mut discarded = db.begin().unwrap();
    discarded.delete_link(64).unwrap();
    discarded.rollback();
    assert!(Arc::ptr_eq(&current.rows, &db.snapshot().unwrap().rows));
    let log = db.with_log(|log| Ok(log.clone())).unwrap();
    let reopened = Engine::<u64, _>::open(log, shared()).unwrap();
    let a = reopened.snapshot_at(1).unwrap();
    let b = reopened.snapshot_at(2).unwrap();
    unmaterialized(&a);
    unmaterialized(&b);
    assert!(
        std::ptr::eq(a.get(64).unwrap(), b.get(64).unwrap()),
        "replayed retained revisions share unchanged entries"
    );
    let branch = reopened.fork(MemoryLog::default(), shared()).unwrap();
    unmaterialized(&branch.engine().snapshot().unwrap());
    let base = branch.merge_base_record();
    let log = branch.into_engine().into_log().ok().unwrap();
    let mut resumed = reopened.resume_branch(log, shared(), &base).unwrap();
    resumed.merge().unwrap();
    unmaterialized(&reopened.snapshot().unwrap());
}

#[test]
fn conversions_and_mutations_invalidate_only_the_new_candidate_cache() {
    let row = Row {
        link: GenericLink::new(1u64, 0, 0),
        name: Some("before".into()),
    };
    let mut rows = Rows::new(StateStructure::Shared);
    rows.insert(1, row.clone());
    assert_eq!(rows.materialize().get(&1), Some(&row));
    let mut candidate = rows.clone();
    candidate.insert(
        2,
        Row {
            link: GenericLink::new(2, 1, 0),
            name: None,
        },
    );
    assert_eq!(rows.materialize().len(), 1);
    assert_eq!(candidate.materialize().len(), 2);
    candidate.remove(&1);
    assert!(!candidate.materialize().contains_key(&1));
    assert!(rows.materialize().contains_key(&1));
    let flat = candidate.adapt(StateStructure::Flat, None);
    assert_eq!(flat.materialize().len(), 1);
    let shared = flat.adapt(StateStructure::Shared, None);
    assert!(shared.get(&1).is_none());
    assert!(shared.get(&2).is_some());
    let empty = Rows::Flat(BTreeMap::new()).adapt(StateStructure::Shared, Some(&shared));
    assert!(empty.is_empty());
    let not_materialized =
        Rows::<u64>::new(StateStructure::Shared).adapt(StateStructure::Flat, None);
    assert!(not_materialized.is_empty());
}
