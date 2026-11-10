use super::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn missing_changed_count_and_oversized_chunks_fail_closed() {
    for kind in [
        "missing",
        "generation",
        "contents",
        "count",
        "bytes",
        "absent-records",
    ] {
        let catalog = Store::default();
        let data = Store::default();
        let mut store = PartitionedJournalStore::new(
            catalog.clone(),
            BTreeMap::from([("east".into(), data.clone())]),
            |_| "east".into(),
        )
        .unwrap();
        let records = if kind == "bytes" {
            vec!["x".repeat(8388609)]
        } else {
            vec!["one".into()]
        };
        let keys = vec!["a".into()];
        let base = store.read(&keys).unwrap();
        assert!(store
            .compare_exchange(&keys, &base, &[Some(records)])
            .unwrap());
        let pointer = catalog.0.lock().unwrap().values["a"].clone();
        let key = pointer.entries[2].clone();
        let generation = pointer.entries[3].clone();
        let count = if kind == "count" { "2" } else { "1" };
        catalog.0.lock().unwrap().values.insert(
            "a".into(),
            JournalVersion {
                generation: pointer.generation,
                entries: vec![
                    "chunks1".into(),
                    "east".into(),
                    count.into(),
                    "1".into(),
                    key.clone(),
                    generation,
                    count.into(),
                ],
            },
        );
        {
            let mut state = data.0.lock().unwrap();
            match kind {
                "missing" => {
                    state.values.remove(&key);
                }
                "generation" => state.values.get_mut(&key).unwrap().generation = 99,
                "contents" => state.values.get_mut(&key).unwrap().entries = vec!["other".into()],
                "absent-records" => state.values.get_mut(&key).unwrap().generation = 0,
                _ => {}
            }
        }
        assert!(store.read(&keys).is_err(), "accepted {kind}");
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_cached_object_does_not_bypass_manifest_counts() {
    let (catalog, data, store) = fixture(JournalChunkOptions::default());
    let mut store = store.with_object_cache(256);
    let keys = vec!["a".into()];
    let base = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &base, &[Some(vec!["one".into()])])
        .unwrap());
    store.read(&keys).unwrap();
    let reads = data.0.lock().unwrap().reads;
    {
        let mut state = catalog.0.lock().unwrap();
        let item = state.values.get_mut("a").unwrap();
        item.entries[2] = "2".into();
        item.entries[6] = "2".into();
    }
    assert!(store.read(&keys).is_err());
    assert_eq!(reads, data.0.lock().unwrap().reads);
}
