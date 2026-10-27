use super::*;

fn layout(chunked: bool) -> (Store, Store, Store, Reclaiming) {
    let c = Store::default();
    let a = Store::default();
    let b = Store::default();
    let store = if chunked {
        chunks::open_chunks(&c, &a, &b, 0)
    } else {
        open(&c, &a, &b, Default::default())
    };
    (c, a, b, store)
}
fn calls(providers: &[&Store]) -> Vec<usize> {
    providers
        .iter()
        .map(|p| p.0.lock().unwrap().calls.len())
        .collect()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn mismatched_arrays_reject_before_io_and_preserve_following_writes() {
    for chunked in [false, true] {
        let (c, a, b, mut store) = layout(chunked);
        let expected = write(&mut store, "old-a", "old-b");
        let before = calls(&[&c, &a, &b]);
        for (guards, updates) in [(1, 2), (3, 2), (2, 1), (2, 3)] {
            let guards = vec![expected[0].clone(); guards];
            let updates = vec![Some(vec!["must-not-stage".into()]); updates];
            assert!(store.compare_exchange(&keys(), &guards, &updates).is_err());
            assert_eq!(calls(&[&c, &a, &b]), before);
        }
        assert_eq!(store.read(&keys()).unwrap(), expected);
        write(&mut store, "following-a", "following-b");
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn generation_zero_with_payload_never_becomes_an_absent_value() {
    for chunked in [false, true] {
        for phase in ["shard", "epoch", "object"] {
            let (c, a, b, mut store) = layout(chunked);
            let expected = write(&mut store, "live-a", "live-b");
            let object = a.0.lock().unwrap().values.keys().next().unwrap().clone();
            let (provider, key) = match phase {
                "shard" => (&c, "a"),
                "epoch" => (&c, EPOCH),
                _ => (&a, object.as_str()),
            };
            let saved = provider.0.lock().unwrap().values[key].clone();
            provider
                .0
                .lock()
                .unwrap()
                .values
                .get_mut(key)
                .unwrap()
                .generation = 0;
            let reads = a.count("read");
            let swaps = c.count("swap");
            assert!(store.read(&keys()).is_err(), "{phase}");
            assert_eq!(a.count("read") - reads, usize::from(phase == "object"));
            assert_eq!(c.count("swap"), swaps);
            assert_eq!(a.count("delete") + b.count("delete"), 0);
            provider.0.lock().unwrap().values.insert(key.into(), saved);
            assert_eq!(store.read(&keys()).unwrap(), expected);
            write(&mut store, "following-a", "following-b");
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn noncanonical_object_keys_reject_before_download_or_deletion() {
    for chunked in [false, true] {
        for position in [6, 21, 23, 38] {
            let (c, a, b, mut store) = layout(chunked);
            // Leave a real old-epoch orphan eligible for deletion, so a
            // malformed later object cannot pass a vacuous no-delete check.
            write(&mut store, "orphan-a", "orphan-b");
            let expected = write(&mut store, "live-a", "live-b");
            let original = c.0.lock().unwrap().values["a"].clone();
            let key_index = if chunked { 4 } else { 2 };
            let mut malformed = original.clone();
            malformed.entries[key_index].replace_range(position..position + 1, "F");
            c.0.lock().unwrap().values.insert("a".into(), malformed);
            let reads = a.count("read") + b.count("read");
            assert!(store.read(&keys()).is_err());
            assert_eq!(a.count("read") + b.count("read"), reads);
            assert!(store.collect_garbage().is_err());
            assert_eq!(a.count("delete") + b.count("delete"), 0);
            c.0.lock()
                .unwrap()
                .values
                .insert("a".into(), original.clone());
            assert_eq!(store.read(&keys()).unwrap(), expected);
            // An unreferenced malformed object must also stop the collector
            // before it deletes any otherwise eligible object.
            let good_key = &original.entries[key_index];
            let value = a.0.lock().unwrap().values[good_key].clone();
            let mut bad_key = good_key.clone();
            bad_key.replace_range(position..position + 1, "F");
            a.0.lock().unwrap().values.insert(bad_key.clone(), value);
            assert!(store.collect_garbage().is_err());
            assert_eq!(a.count("delete") + b.count("delete"), 0);
            a.0.lock().unwrap().values.remove(&bad_key);
            write(&mut store, "following-a", "following-b");
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unchanged_pointer_cannot_turn_a_missing_object_into_a_refresh() {
    for chunked in [false, true] {
        let (c, a, b, mut store) = layout(chunked);
        let expected = write(&mut store, "live-a", "live-b");
        let original = c.0.lock().unwrap().values["a"].clone();
        let object_key = a.0.lock().unwrap().values.keys().next().unwrap().clone();
        let object = a.0.lock().unwrap().values[&object_key].clone();
        let catalog = c.clone();
        let data = a.clone();
        let removed_key = object_key.clone();
        a.on("read", false, move || {
            assert!(catalog.clone().compare_exchange(
                &["a".into()],
                std::slice::from_ref(&original),
                &[Some(original.entries.clone())]
            )?);
            data.0.lock().unwrap().values.remove(&removed_key);
            Ok(())
        });
        let reads = (c.count("read"), a.count("read"), b.count("read"));
        assert!(store.read(&keys()).is_err());
        assert_eq!(
            (c.count("read"), a.count("read"), b.count("read")),
            (reads.0 + 2, reads.1 + 1, reads.2)
        );
        a.0.lock().unwrap().values.insert(object_key, object);
        let fresh = store.read(&keys()).unwrap();
        assert_eq!(fresh[0].entries, expected[0].entries);
        assert_ne!(fresh[0].generation, expected[0].generation);
        write(&mut store, "following-a", "following-b");
    }
}
