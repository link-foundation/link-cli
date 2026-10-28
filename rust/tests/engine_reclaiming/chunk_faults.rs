use super::chunks::open_chunks;
use super::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn retired_suffix_discards_cached_and_fetched_prefixes_and_other_shards() {
    for cache in [0, 256] {
        let c = Store::default();
        let a = Store::default();
        let b = Store::default();
        let mut writer = open_chunks(&c, &a, &b, 0);
        let mut reader = open_chunks(&c, &a, &b, cache);
        let old = [
            Some(vec!["a0".into(), "a0-tail".into(), "a0-last".into()]),
            Some(vec!["b0".into(), "b0-tail".into(), "b0-last".into()]),
        ];
        let base = writer.read(&keys()).unwrap();
        assert!(writer.compare_exchange(&keys(), &base, &old).unwrap());
        if cache > 0 {
            reader.read(&["a".into()]).unwrap();
        }
        let base = writer.read(&keys()).unwrap();
        let fresh: Vec<_> = [
            vec!["a1".into(), "a1-tail".into(), "a1-last".into()],
            vec!["b1".into(), "b1-tail".into(), "b1-last".into()],
        ]
        .into_iter()
        .map(Some)
        .collect();
        let expected = fresh.clone();
        b.on("read", false, || Ok(()));
        b.on("read", false, move || {
            assert!(writer.compare_exchange(&keys(), &base, &fresh)?);
            assert_eq!(writer.collect_garbage()?.unwrap().deleted, 4);
            Ok(())
        });
        let group = reader.read(&keys()).unwrap();
        for (actual, expected) in group.iter().zip(expected) {
            assert_eq!(actual.entries, expected.unwrap());
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_chunk_staging_and_publication_cut_preserves_complete_groups() {
    for cut in 0..5 {
        for after in [false, true] {
            let c = Store::default();
            let a = Store::default();
            let b = Store::default();
            let mut store = open_chunks(&c, &a, &b, 0);
            let old = write(&mut store, "old-a", "old-b");
            let updates = [
                Some(vec!["a".into(), "b".into(), "c".into()]),
                Some(vec!["d".into(), "e".into(), "f".into()]),
            ];
            let provider = if cut < 2 {
                &a
            } else if cut < 4 {
                &b
            } else {
                &c
            };
            if cut == 1 || cut == 3 {
                provider.on("swap", after, || Ok(()));
            }
            provider.on("swap", after, || Err(error("finite chunk cut")));
            assert!(store.compare_exchange(&keys(), &old, &updates).is_err());
            let fresh = store.read(&keys()).unwrap();
            for i in 0..2 {
                assert_eq!(
                    fresh[i].entries,
                    if after && cut == 4 {
                        updates[i].clone().unwrap()
                    } else {
                        old[i].entries.clone()
                    }
                );
            }
            assert!(store
                .compare_exchange(&keys(), &fresh, &[Some(vec!["following".into()]), None])
                .unwrap());
            assert!(!store
                .compare_exchange(&keys(), &old, &[Some(vec!["stale".into()]), None])
                .unwrap());
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn fence_after_final_chunk_staging_conflicts_without_replaying_publication() {
    let c = Store::default();
    let a = Store::default();
    let b = Store::default();
    let mut writer = open_chunks(&c, &a, &b, 0);
    let mut collector = open_chunks(&c, &a, &b, 0);
    let selected = vec!["a".into()];
    let before = writer.read(&selected).unwrap();
    let swaps = c.count("swap");
    c.on("swap", false, move || {
        assert_eq!(collector.collect_garbage()?.unwrap().epoch, 1);
        Ok(())
    });
    assert!(!writer
        .compare_exchange(
            &selected,
            &before,
            &[Some(vec!["a".into(), "b".into(), "c".into()])]
        )
        .unwrap());
    assert_eq!(c.count("swap") - swaps, 2);
    assert_eq!(writer.read(&selected).unwrap(), before);
    assert!(writer
        .compare_exchange(&selected, &before, &[Some(vec!["following".into()])])
        .unwrap());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn cached_counts_actual_generations_and_fresh_staging_are_verified() {
    for kind in [
        "count",
        "cached-count",
        "generation",
        "contents",
        "missing",
        "staging",
    ] {
        let c = Store::default();
        let a = Store::default();
        let b = Store::default();
        let mut store = open_chunks(
            &c,
            &a,
            &b,
            if kind == "cached-count" || kind == "staging" {
                256
            } else {
                0
            },
        );
        let selected = vec!["a".into()];
        let base = store.read(&selected).unwrap();
        assert!(store
            .compare_exchange(&selected, &base, &[Some(vec!["one".into()])])
            .unwrap());
        let owned = store.read(&selected).unwrap();
        let key = c.0.lock().unwrap().values["a"].entries[4].clone();
        if kind.contains("count") {
            let mut state = c.0.lock().unwrap();
            let pointer = state.values.get_mut("a").unwrap();
            pointer.entries[2] = "2".into();
            pointer.entries[6] = "2".into();
        }
        if kind == "generation" {
            a.0.lock().unwrap().values.get_mut(&key).unwrap().generation = 99;
        }
        if kind == "contents" || kind == "staging" {
            a.0.lock().unwrap().values.get_mut(&key).unwrap().entries = vec!["damaged".into()];
        }
        if kind == "missing" {
            a.0.lock().unwrap().values.remove(&key);
        }
        let reads = a.count("read");
        if kind == "staging" {
            assert!(store
                .compare_exchange(&selected, &owned, &[Some(vec!["one".into()])])
                .is_err());
        } else {
            assert!(store.read(&selected).is_err());
        }
        if kind == "cached-count" {
            assert_eq!(a.count("read"), reads);
        }
    }
}
