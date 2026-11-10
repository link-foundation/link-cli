use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn finite_edits_match_an_independent_map_and_keep_every_pinned_view() {
    let mut runs = SortedRuns::new();
    let mut oracle = BTreeMap::new();
    let mut saved = Vec::new();
    for step in 0..1024u64 {
        let id = (step * 17) % 31;
        let value = (step % 5 != 0).then_some(step);
        runs.set(id, value);
        if let Some(value) = value {
            oracle.insert(id, value);
        } else {
            oracle.remove(&id);
        }
        for id in 0..=32 {
            assert_eq!(runs.get(&id), oracle.get(&id));
        }
        assert_eq!(runs.len, oracle.len());
        assert_eq!(runs.ordered(), &oracle);
        assert!(runs.levels.len() <= 11);
        for run in runs.levels.iter().flatten() {
            assert!(run.windows(2).all(|pair| pair[0].0 < pair[1].0));
        }
        saved.push((runs.clone(), oracle.clone()));
    }
    for (snapshot, expected) in saved {
        assert_eq!(snapshot.ordered(), &expected);
        assert_eq!(snapshot.into_ordered(), expected);
    }
}

#[test]
fn tombstones_hide_older_values_and_disappear_only_after_complete_compaction() {
    let mut runs = SortedRuns::new();
    runs.set(1, Some(11));
    runs.set(2, Some(22));
    let pinned = runs.clone();
    runs.set(1, None);
    assert_eq!(runs.get(&1), None);
    assert!(runs.levels[0].as_ref().unwrap().contains(&(1, None)));
    assert!(!runs.ordered().contains_key(&1));
    runs.set(2, None);
    assert!(runs.levels.is_empty());
    assert_eq!(runs.len, 0);
    assert_eq!(pinned.get(&1), Some(&11));
    assert_eq!(pinned.get(&2), Some(&22));
    runs.set(1, Some(33));
    assert_eq!(runs.get(&1), Some(&33));
    assert_eq!(runs.ordered(), &BTreeMap::from([(1, 33)]));
}

#[test]
fn no_ops_preserve_runs_and_an_existing_scan_cache() {
    let mut runs = SortedRuns::new();
    runs.set(3, Some(30));
    let root = runs.levels[0].as_ref().unwrap().clone();
    let cached = runs.ordered() as *const _;
    runs.set(3, Some(30));
    runs.set(4, None);
    assert!(Arc::ptr_eq(&root, runs.levels[0].as_ref().unwrap()));
    assert!(runs.ordered.get().is_some());
    assert_eq!(cached, runs.ordered() as *const _);
    let old = runs.clone();
    runs.set(3, Some(31));
    assert!(runs.ordered.get().is_none());
    assert_eq!(old.get(&3), Some(&30));
    assert_eq!(runs.get(&3), Some(&31));
}

struct Counted(u64, Arc<AtomicUsize>);
impl Clone for Counted {
    fn clone(&self) -> Self {
        self.1.fetch_add(1, Ordering::Relaxed);
        Self(self.0, self.1.clone())
    }
}
impl PartialEq for Counted {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[test]
fn snapshots_and_point_reads_copy_no_values_and_share_immutable_runs() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<SortedRuns<u64, u64>>();
    let copies = Arc::new(AtomicUsize::new(0));
    let mut runs = SortedRuns::new();
    for id in 0..1023 {
        runs.set(id, Some(Counted(id, copies.clone())));
    }
    copies.store(0, Ordering::Relaxed);
    let snapshot = runs.clone();
    for (a, b) in runs.levels.iter().zip(&snapshot.levels) {
        if let (Some(a), Some(b)) = (a, b) {
            assert!(Arc::ptr_eq(a, b));
        }
    }
    assert_eq!(snapshot.get(&100).unwrap().0, 100);
    assert_eq!(copies.load(Ordering::Relaxed), 0);
    assert!(snapshot.ordered.get().is_none());
}

#[test]
fn merge_orders_interleaved_keys_and_newest_tombstones_win() {
    let old = vec![(1, Some(10)), (3, Some(30)), (5, None), (9, Some(90))];
    let new = vec![(0, Some(0)), (3, None), (5, Some(50)), (7, Some(70))];
    assert_eq!(
        SortedRuns::merge(&old, new),
        vec![
            (0, Some(0)),
            (1, Some(10)),
            (3, None),
            (5, Some(50)),
            (7, Some(70)),
            (9, Some(90)),
        ]
    );
    assert!(SortedRuns::<u64, u64>::merge(&[], vec![]).is_empty());
    assert!(SortedRuns::<u64, u64>::new().into_ordered().is_empty());
}
