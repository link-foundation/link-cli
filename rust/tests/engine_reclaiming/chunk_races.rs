use super::chunks::open_chunks;
use super::*;
fn publish(store: &mut Reclaiming, entries: &[&str]) {
    let selected = vec!["a".into()];
    let base = store.read(&selected).unwrap();
    assert!(store
        .compare_exchange(
            &selected,
            &base,
            &[Some(entries.iter().map(|s| (*s).into()).collect())]
        )
        .unwrap());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn second_chunk_retirement_stops_after_one_whole_group_retry() {
    let c = Store::default();
    let a = Store::default();
    let b = Store::default();
    let mut writer = open_chunks(&c, &a, &b, 0);
    let mut second = open_chunks(&c, &a, &b, 0);
    let mut reader = open_chunks(&c, &a, &b, 0);
    publish(&mut writer, &["zero", "zero-tail", "zero-last"]);
    let data = a.clone();
    a.on("read", false, move || {
        publish(&mut writer, &["one", "one-tail", "one-last"]);
        writer.collect_garbage()?;
        data.on("read", false, move || {
            publish(&mut second, &["two", "two-tail", "two-last"]);
            second.collect_garbage()?;
            Ok(())
        });
        Ok(())
    });
    let keys = vec!["a".into()];
    assert!(reader.read(&keys).is_err());
    assert_eq!(
        reader.read(&keys).unwrap()[0].entries,
        ["two", "two-tail", "two-last"]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn own_collector_fence_preserves_all_newer_orphan_chunks() {
    let c = Store::default();
    let a = Store::default();
    let b = Store::default();
    let mut writer = open_chunks(&c, &a, &b, 0);
    let mut collector = open_chunks(&c, &a, &b, 0);
    let mut other = open_chunks(&c, &a, &b, 0);
    publish(&mut writer, &["live", "tail", "last"]);
    let catalog = c.clone();
    let data = a.clone();
    c.on("scan", false, move || {
        catalog.on("swap", false, || Err(error("lost publication")));
        let keys = vec!["a".into()];
        let base = writer.read(&keys)?;
        assert!(writer
            .compare_exchange(
                &keys,
                &base,
                &[Some(vec!["orphan".into(), "tail".into(), "last".into()])]
            )
            .is_err());
        data.on("scan", false, || Err(error("incomplete next collector")));
        assert!(other.collect_garbage().is_err());
        Ok(())
    });
    let result = collector.collect_garbage().unwrap().unwrap();
    assert_eq!((result.epoch, result.examined, result.deleted), (1, 4, 0));
    assert_eq!(
        a.0.lock()
            .unwrap()
            .values
            .keys()
            .filter(|k| k.starts_with("blob2-0000000000000001-"))
            .count(),
        2
    );
    assert_eq!(collector.collect_garbage().unwrap().unwrap().deleted, 2);
    assert_eq!(
        collector.read(&["a".into()]).unwrap()[0].entries,
        ["live", "tail", "last"]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn complete_later_namespace_and_exact_generation_guard_chunk_deletion() {
    let c = Store::default();
    let a = Store::default();
    let b = Store::default();
    let mut store = open_chunks(&c, &a, &b, 0);
    publish(&mut store, &["old", "tail", "last"]);
    let key = c.0.lock().unwrap().values["a"].entries[4].clone();
    publish(&mut store, &["live", "fresh", "end"]);
    b.on("scan", false, || Err(error("incomplete later namespace")));
    assert!(store.collect_garbage().is_err());
    assert_eq!(a.count("delete"), 0);
    let data = a.clone();
    let saved = key.clone();
    a.on("delete", false, move || {
        let mut state = data.0.lock().unwrap();
        state.revision += 1;
        let generation = state.revision;
        state.values.insert(
            saved,
            JournalVersion {
                generation,
                entries: vec!["ABA replacement".into()],
            },
        );
        Ok(())
    });
    assert_eq!(store.collect_garbage().unwrap().unwrap().deleted, 1);
    assert_eq!(
        a.0.lock().unwrap().values[&key].entries,
        ["ABA replacement"]
    );
    assert_eq!(
        store.read(&["a".into()]).unwrap()[0].entries,
        ["live", "fresh", "end"]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn mixed_whole_and_live_empty_chunk_roots_preserve_following_edits() {
    let c = Store::default();
    let a = Store::default();
    let b = Store::default();
    let mut legacy = open(&c, &a, &b, Default::default());
    publish(&mut legacy, &["legacy"]);
    let mut store = open_chunks(&c, &a, &b, 0);
    let keys = vec!["a".into()];
    let owned = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &owned, &[Some(vec![])])
        .unwrap());
    assert_eq!(
        c.0.lock().unwrap().values["a"].entries,
        ["chunks2", "east", "0", "0"]
    );
    assert_eq!(store.collect_garbage().unwrap().unwrap().deleted, 1);
    assert!(a.0.lock().unwrap().values.is_empty());
    assert!(legacy.read(&keys).unwrap()[0].entries.is_empty());
    assert!(!store
        .compare_exchange(&keys, &owned, &[Some(vec!["stale".into()])])
        .unwrap());
    publish(&mut store, &["following", "tail"]);
    assert_eq!(
        legacy.read(&keys).unwrap()[0].entries,
        ["following", "tail"]
    );
}
