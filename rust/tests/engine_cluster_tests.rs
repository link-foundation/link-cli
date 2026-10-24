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
    short_read: bool,
    panic_read: bool,
    durable: bool,
}
impl AtomicJournalStore for Store {
    fn supports_durable_flush(&self) -> bool {
        self.0.lock().unwrap().durable
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        let state = self.0.lock().unwrap();
        assert!(!state.panic_read, "injected coordinator panic");
        if state.short_read {
            return Ok(vec![]);
        }
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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_policies_and_coordinates_are_rejected_and_constraints_publish_no_shards() {
    use link_cli::engine::{ConcurrencyControl, Constraint, IsolationLevel};
    for options in [
        EngineOptions {
            concurrency: ConcurrencyControl::Exclusive,
            ..Default::default()
        },
        EngineOptions {
            concurrency: ConcurrencyControl::Pessimistic,
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn raw_links_api_is_private_and_closed_with_its_group() {
    let cluster = ClusterEngine::<u32, _>::new(Store::default(), EngineOptions::default()).unwrap();
    let mut group = cluster.begin(&["a"]).unwrap();
    let shard = group.shard("a").unwrap();
    assert_eq!(shard.ensure_link_created(10).unwrap(), 10);
    assert_eq!(shard.create_link(10, 0).unwrap(), 11);
    assert_eq!(shard.update_link(10, 11, 0).unwrap().source, 0);
    shard.set_name(10, Some("雪".into())).unwrap();
    assert_eq!(shard.get_by_name("雪"), Some(10));
    shard.set_name(10, None).unwrap();
    assert_eq!(shard.get_by_name("雪"), None);
    assert_eq!(shard.all_links().len(), 2);
    assert_eq!(shard.delete_link(11).unwrap().index, 11);
    assert!(cluster
        .begin(&["a"])
        .unwrap()
        .shard("a")
        .unwrap()
        .all_links()
        .is_empty());
    group.commit().unwrap();
    assert!(group.shard("a").is_err());
    assert_eq!(
        cluster
            .begin(&["a"])
            .unwrap()
            .shard("a")
            .unwrap()
            .get_link(10)
            .unwrap()
            .source,
        11
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn durable_backend_and_boundary_names_are_accepted_but_bad_snapshots_fail_closed() {
    let store = Store::default();
    store.0.lock().unwrap().durable = true;
    let cluster = ClusterEngine::<u32, _>::new(
        store.clone(),
        EngineOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    )
    .unwrap();
    for name in ["a.b_c-d", &"a".repeat(128)] {
        cluster.begin(&[name]).unwrap().commit().unwrap();
    }
    for name in ["", &"a".repeat(129)] {
        assert!(cluster.begin(&[name]).is_err());
    }
    store.0.lock().unwrap().short_read = true;
    assert!(cluster
        .begin(&["a"])
        .err()
        .unwrap()
        .to_string()
        .contains("incomplete"));
    store.0.lock().unwrap().short_read = false;
    store.0.lock().unwrap().values.insert(
        "bad".into(),
        JournalVersion {
            generation: 1,
            entries: vec!["corrupt".into()],
        },
    );
    assert!(cluster.begin(&["a", "bad"]).is_err());
    cluster.begin(&["a"]).unwrap().commit().unwrap();
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg(not(target_arch = "wasm32"))]
fn panicked_coordinator_rejects_new_groups_and_pending_publication() {
    let store = Store::default();
    let cluster = ClusterEngine::<u32, _>::new(store.clone(), EngineOptions::default()).unwrap();
    let mut group = cluster.begin(&["a"]).unwrap();
    group.shard("a").unwrap().create_link(0, 0).unwrap();
    store.0.lock().unwrap().panic_read = true;
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cluster.begin(&["a"])));
    assert!(panic.is_err());
    assert!(cluster
        .begin(&["a"])
        .err()
        .unwrap()
        .to_string()
        .contains("poisoned"));
    assert!(group.commit().unwrap_err().to_string().contains("poisoned"));
}
