use super::*;
use link_cli::engine::{partitioned::PartitionedJournalStore, Engine, HistoryPolicy, MemoryLog};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn selected_checkpoint_shrinks_and_equal_length_writes_still_publish() {
    for history in [
        HistoryPolicy::None,
        HistoryPolicy::Last(2),
        HistoryPolicy::All,
    ] {
        let store = Store::default();
        let cluster = ClusterEngine::<u32, _>::new(
            store.clone(),
            EngineOptions {
                history,
                ..Default::default()
            },
        )
        .unwrap()
        .with_journal_checkpoint_interval(2)
        .unwrap();
        assert!(
            ClusterEngine::<u32, _>::new(store.clone(), EngineOptions::default())
                .unwrap()
                .with_journal_checkpoint_interval(0)
                .is_err()
        );
        for revision in 1..=6 {
            let mut tx = cluster.begin(&["a", "b"]).unwrap();
            for key in ["a", "b"] {
                if revision == 1 {
                    tx.shard(key).unwrap().create_link(0, 0).unwrap();
                }
                tx.shard(key)
                    .unwrap()
                    .set_name(1, Some(format!("{key} {revision}")))
                    .unwrap();
            }
            tx.commit().unwrap();
            let records = store.0.lock().unwrap().values["a"].entries.clone();
            if revision % 2 == 0 {
                assert!(records[0].starts_with("engine3|"));
                assert_eq!(
                    records.len(),
                    match history {
                        HistoryPolicy::None => 1,
                        HistoryPolicy::Last(_) => 2,
                        HistoryPolicy::All => revision as usize + 1,
                    }
                );
            }
            let reopened = Engine::<u32, _>::open(
                MemoryLog { entries: records },
                EngineOptions {
                    history: HistoryPolicy::All,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(reopened.snapshot().unwrap().revision, revision);
            assert_eq!(
                reopened.snapshot().unwrap().get(1).unwrap().name,
                Some(format!("a {revision}"))
            );
            if history != HistoryPolicy::All && revision >= 2 {
                assert!(reopened.snapshot_at(0).is_err());
            }
        }
        let before = store.0.lock().unwrap().values.clone();
        cluster.begin(&["a", "b"]).unwrap().commit().unwrap();
        assert_eq!(before, store.0.lock().unwrap().values);
    }
    let store = Store::default();
    let default = ClusterEngine::<u32, _>::new(store.clone(), EngineOptions::default()).unwrap();
    for id in 1..=3 {
        let mut tx = default.begin(&["a"]).unwrap();
        assert_eq!(tx.shard("a").unwrap().create_link(0, 0).unwrap(), id);
        tx.commit().unwrap();
    }
    assert_eq!(store.0.lock().unwrap().values["a"].entries.len(), 3);
    assert!(store.0.lock().unwrap().values["a"].entries[0].starts_with("engine1|"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn compacted_groups_keep_conflicts_and_before_after_unknown_outcomes() {
    for after in [false, true] {
        let store = Store::default();
        let cluster = ClusterEngine::<u32, _>::new(store.clone(), EngineOptions::default())
            .unwrap()
            .with_journal_checkpoint_interval(1)
            .unwrap();
        let mut seed = cluster.begin(&["a", "b"]).unwrap();
        seed.shard("a").unwrap().create_link(0, 0).unwrap();
        seed.shard("b").unwrap().create_link(0, 0).unwrap();
        seed.commit().unwrap();
        let mut stale = cluster.begin(&["a", "b"]).unwrap();
        let mut fresh = cluster.begin(&["b"]).unwrap();
        fresh
            .shard("b")
            .unwrap()
            .set_name(1, Some("fresh".into()))
            .unwrap();
        fresh.commit().unwrap();
        stale.shard("a").unwrap().delete_link(1).unwrap();
        assert!(stale.commit().unwrap_err().to_string().contains("conflict"));
        let mut unknown = cluster.begin(&["a", "b"]).unwrap();
        for key in ["a", "b"] {
            unknown
                .shard(key)
                .unwrap()
                .set_name(1, Some("unknown".into()))
                .unwrap();
        }
        {
            let mut state = store.0.lock().unwrap();
            state.lose_reply = after;
            state.fail_before = !after;
        }
        assert!(unknown
            .commit()
            .unwrap_err()
            .to_string()
            .contains("indeterminate"));
        assert!(unknown.commit().is_err());
        assert!(unknown.shard("a").is_err());
        {
            let mut state = store.0.lock().unwrap();
            state.lose_reply = false;
            state.fail_before = false;
        }
        let mut check = cluster.begin(&["a", "b"]).unwrap();
        assert!(check.shard("a").unwrap().get_link(1).is_some());
        assert_eq!(
            check.shard("a").unwrap().get_name(1).as_deref(),
            after.then_some("unknown")
        );
        assert_eq!(
            check.shard("b").unwrap().get_name(1).as_deref(),
            Some(if after { "unknown" } else { "fresh" })
        );
        check
            .shard("a")
            .unwrap()
            .set_name(1, Some("following".into()))
            .unwrap();
        check.commit().unwrap();
        assert_eq!(
            cluster
                .begin(&["a"])
                .unwrap()
                .shard("a")
                .unwrap()
                .get_name(1)
                .as_deref(),
            Some("following")
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn partitioned_checkpoints_publish_new_objects_without_mutating_old_objects() {
    let catalog = Store::default();
    let data = Store::default();
    let partitioned = PartitionedJournalStore::new(
        catalog,
        BTreeMap::from([("data".into(), data.clone())]),
        |_| "data".into(),
    )
    .unwrap()
    .with_object_cache(4096);
    let cluster = ClusterEngine::<u32, _>::new(partitioned, EngineOptions::default())
        .unwrap()
        .with_journal_checkpoint_interval(1)
        .unwrap();
    let mut seed = cluster.begin(&["a"]).unwrap();
    seed.shard("a").unwrap().create_link(0, 0).unwrap();
    seed.commit().unwrap();
    let original = data.0.lock().unwrap().values.clone();
    let mut next = cluster.begin(&["a"]).unwrap();
    next.shard("a")
        .unwrap()
        .set_name(1, Some("next".into()))
        .unwrap();
    next.commit().unwrap();
    for (key, value) in original {
        assert_eq!(data.0.lock().unwrap().values[&key], value);
    }
    assert_eq!(data.0.lock().unwrap().values.len(), 2);
    assert_eq!(
        cluster
            .begin(&["a"])
            .unwrap()
            .shard("a")
            .unwrap()
            .get_name(1)
            .as_deref(),
        Some("next")
    );
}
