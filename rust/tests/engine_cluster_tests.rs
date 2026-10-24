use link_cli::engine::{
    cluster::{AtomicJournalStore, ClusterEngine, JournalVersion},
    EngineOptions,
};
use link_cli::{LinkError, LinksStorage};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct Store(Arc<Mutex<State>>);
#[derive(Default)]
struct State {
    revision: u64,
    values: BTreeMap<String, JournalVersion>,
    lose_reply: bool,
}
impl AtomicJournalStore for Store {
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        let state = self.0.lock().unwrap();
        Ok(keys
            .iter()
            .map(|key| state.values.get(key).cloned().unwrap_or_default())
            .collect())
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        let mut state = self.0.lock().unwrap();
        if keys.iter().zip(expected).any(|(key, value)| {
            state.values.get(key).map_or(0, |v| v.generation) != value.generation
        }) {
            return Ok(false);
        }
        state.revision += 1;
        let generation = state.revision;
        for (key, update) in keys.iter().zip(updates) {
            if let Some(entries) = update {
                state.values.insert(
                    key.clone(),
                    JournalVersion {
                        generation,
                        entries: entries.clone(),
                    },
                );
            }
        }
        if state.lose_reply {
            return Err(LinkError::Transaction("response lost".into()));
        }
        Ok(true)
    }
}
#[test]
fn cross_shard_commit_is_atomic_and_checks_read_only_shards() {
    let store = Store::default();
    let first = ClusterEngine::<u32, _>::new(store.clone(), EngineOptions::default()).unwrap();
    let second = ClusterEngine::<u32, _>::new(store, EngineOptions::default()).unwrap();
    let mut seed = first.begin(&["a", "b"]).unwrap();
    seed.shard("a").unwrap().create_link(0, 0).unwrap();
    seed.shard("b").unwrap().create_link(0, 0).unwrap();
    seed.commit().unwrap();
    let mut stale = first.begin(&["a", "b"]).unwrap();
    let mut fresh = second.begin(&["b"]).unwrap();
    fresh
        .shard("b")
        .unwrap()
        .set_name(1, Some("fresh".into()))
        .unwrap();
    fresh.commit().unwrap();
    stale.shard("a").unwrap().delete_link(1).unwrap();
    assert!(stale.commit().unwrap_err().to_string().contains("conflict"));
    assert!(stale.shard("a").is_err());
    let mut check = second.begin(&["a", "b"]).unwrap();
    assert!(check.shard("a").unwrap().get_link(1).is_some());
    assert_eq!(
        check.shard("b").unwrap().get_name(1).as_deref(),
        Some("fresh")
    );
}
#[test]
fn independent_shards_commit_without_global_false_conflicts_and_rollback_is_private() {
    let cluster = ClusterEngine::<u32, _>::new(Store::default(), EngineOptions::default()).unwrap();
    let mut a = cluster.begin(&["a"]).unwrap();
    let mut b = cluster.begin(&["b"]).unwrap();
    a.shard("a").unwrap().create_link(0, 0).unwrap();
    b.shard("b").unwrap().create_link(0, 0).unwrap();
    a.commit().unwrap();
    b.commit().unwrap();
    let mut private = cluster.begin(&["a", "b"]).unwrap();
    private.shard("a").unwrap().delete_link(1).unwrap();
    private.rollback();
    assert!(cluster
        .begin(&["a"])
        .unwrap()
        .shard("a")
        .unwrap()
        .get_link(1)
        .is_some());
}
#[test]
fn lost_reply_ends_transaction_and_reopen_discovers_all_committed_shards() {
    let store = Store::default();
    let cluster = ClusterEngine::<u32, _>::new(store.clone(), EngineOptions::default()).unwrap();
    let mut tx = cluster.begin(&["a", "b"]).unwrap();
    tx.shard("a").unwrap().create_link(0, 0).unwrap();
    tx.shard("b").unwrap().create_link(0, 0).unwrap();
    store.0.lock().unwrap().lose_reply = true;
    assert!(tx
        .commit()
        .unwrap_err()
        .to_string()
        .contains("indeterminate"));
    assert!(tx.commit().is_err());
    let mut recovered = cluster.begin(&["a", "b"]).unwrap();
    assert_eq!(recovered.shard("a").unwrap().links_count(), 1);
    assert_eq!(recovered.shard("b").unwrap().links_count(), 1);
}

#[test]
fn invalid_policies_and_coordinates_are_rejected_and_constraints_publish_no_shards() {
    use link_cli::engine::{ConcurrencyControl, Constraint, IsolationLevel};
    for options in [
        EngineOptions {
            concurrency: ConcurrencyControl::Exclusive,
            ..Default::default()
        },
        EngineOptions {
            isolation: IsolationLevel::Snapshot,
            ..Default::default()
        },
        EngineOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    ] {
        assert!(ClusterEngine::<u32, _>::new(Store::default(), options).is_err());
    }
    let store = Store::default();
    let cluster = ClusterEngine::<u32, _>::new(
        store.clone(),
        EngineOptions {
            constraints: vec![Constraint::References],
            ..Default::default()
        },
    )
    .unwrap();
    for keys in [vec![], vec!["a", "a"], vec!["../a"], vec!["a"; 33]] {
        assert!(cluster.begin(&keys).is_err());
    }
    let mut tx = cluster.begin(&["a", "b"]).unwrap();
    assert!(tx.shard("absent").is_err());
    tx.shard("a").unwrap().create_link(0, 0).unwrap();
    tx.shard("b").unwrap().create_link(999, 0).unwrap();
    assert!(tx.commit().is_err());
    assert!(tx.commit().is_err());
    assert!(store.0.lock().unwrap().values.is_empty());
}

#[test]
fn selected_shard_supports_the_named_links_api_without_individual_publication() {
    use link_cli::NamedTypeLinks;
    let cluster = ClusterEngine::<u32, _>::new(Store::default(), EngineOptions::default()).unwrap();
    let mut tx = cluster.begin(&["a"]).unwrap();
    let shard = tx.shard("a").unwrap();
    fn exercise(api: &mut impl NamedTypeLinks) {
        let a = api.try_create(0, 0).unwrap();
        assert_eq!(a, 1);
        assert_eq!(api.ensure_created(2), 2);
        assert_eq!(api.try_ensure_created(2).unwrap(), 2);
        assert!(api.exists(a));
        assert!(api.get_link(a).is_some());
        assert_eq!(api.create(0, 0), 3);
        assert_eq!(api.get_or_create(0, 0), 1);
        api.set_name(a, "named").unwrap();
        assert_eq!(api.get_name(a).unwrap().as_deref(), Some("named"));
        assert_eq!(api.get_by_name("named").unwrap(), Some(a));
        api.remove_name(a).unwrap();
        let mut observed = 0;
        api.update_observed(a, 2, 3, &mut |_, _| observed += 1)
            .unwrap();
        assert_eq!(api.search(2, 3), Some(a));
        assert_eq!(api.all_links().len(), 3);
        api.delete_observed(3, &mut |_, _| observed += 1).unwrap();
        assert_eq!(observed, 2);
        api.save().unwrap();
    }
    exercise(shard);
    assert!(LinksStorage::flush(shard).is_err());
    tx.commit().unwrap();
}
