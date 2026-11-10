use super::*;
use link_cli::engine::partitioned::JournalChunkOptions;
pub(super) fn selected() -> JournalChunkOptions {
    JournalChunkOptions {
        max_records: 2,
        max_bytes: 64,
        max_chunks: 32,
    }
}
pub(super) fn open_chunks(c: &Store, a: &Store, b: &Store, cache_bytes: usize) -> Reclaiming {
    Reclaiming::open_chunked(
        c.clone(),
        BTreeMap::from([("east".into(), a.clone()), ("west".into(), b.clone())]),
        |key| {
            if key == "a" {
                "east".into()
            } else {
                "west".into()
            }
        },
        ReclamationOptions {
            cache_bytes,
            ..Default::default()
        },
        selected(),
    )
    .unwrap()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn selected_chunks_reuse_within_epoch_and_restage_after_fence() {
    let c = Store::default();
    let a = Store::default();
    let mut store = Reclaiming::open_chunked(
        c.clone(),
        BTreeMap::from([("east".into(), a.clone())]),
        |_| "east".into(),
        ReclamationOptions {
            cache_bytes: 256,
            ..Default::default()
        },
        JournalChunkOptions {
            max_records: 2,
            max_bytes: 64,
            max_chunks: 32,
        },
    )
    .unwrap();
    let keys = vec!["a".into()];
    let seed: Vec<String> = ["aa", "bb", "cc", "dd"]
        .into_iter()
        .map(String::from)
        .collect();
    let base = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &base, &[Some(seed.clone())])
        .unwrap());
    let original = c.0.lock().unwrap().values["a"].clone();
    assert_eq!(original.entries[0], "chunks2");
    assert_eq!(original.entries[3], "2");
    let extended: Vec<_> = seed.iter().cloned().chain(["ee".into()]).collect();
    let base = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &base, &[Some(extended.clone())])
        .unwrap());
    let appended = c.0.lock().unwrap().values["a"].clone();
    assert_eq!(appended.entries[4..10], original.entries[4..10]);
    assert_eq!(a.count("swap"), 3);
    let owned = store.read(&keys).unwrap();
    assert_eq!(store.collect_garbage().unwrap().unwrap().epoch, 1);
    let fresh: Vec<_> = extended.iter().cloned().chain(["ff".into()]).collect();
    assert!(store
        .compare_exchange(&keys, &owned, &[Some(fresh.clone())])
        .unwrap());
    let root = c.0.lock().unwrap().values["a"].clone();
    for triple in root.entries[4..].as_chunks::<3>().0 {
        assert!(triple[0].starts_with("blob2-0000000000000001-"));
    }
    assert_eq!(owned[0].entries, extended);
    assert_eq!(store.collect_garbage().unwrap().unwrap().deleted, 3);
    assert_eq!(store.read(&keys).unwrap()[0].entries, fresh);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn all_chunk_options_and_updates_validate_before_any_io() {
    for chunks in [
        JournalChunkOptions {
            max_records: 0,
            ..selected()
        },
        JournalChunkOptions {
            max_bytes: 8388609,
            ..selected()
        },
        JournalChunkOptions {
            max_chunks: 4097,
            ..selected()
        },
    ] {
        let c = Store::default();
        let a = Store::default();
        assert!(Reclaiming::open_chunked(
            c.clone(),
            BTreeMap::from([("east".into(), a.clone())]),
            |_| "east".into(),
            Default::default(),
            chunks
        )
        .is_err());
        assert!(c.0.lock().unwrap().calls.is_empty());
        assert!(a.0.lock().unwrap().calls.is_empty());
    }
    let c = Store::default();
    let a = Store::default();
    let mut store = Reclaiming::open_chunked(
        c.clone(),
        BTreeMap::from([("east".into(), a.clone())]),
        |_| "east".into(),
        Default::default(),
        JournalChunkOptions {
            max_records: 1,
            max_bytes: 4,
            max_chunks: 2,
        },
    )
    .unwrap();
    for records in [
        vec!["12345".into()],
        vec!["a".into(), "b".into(), "c".into()],
    ] {
        let n = c.count("read");
        let d = a.count("read");
        assert!(store
            .compare_exchange(
                &keys(),
                &vec![JournalVersion::default(); 2],
                &[Some(vec!["ok".into()]), Some(records)]
            )
            .is_err());
        assert_eq!(c.count("read"), n);
        assert_eq!(a.count("read"), d);
        assert_eq!(a.count("swap"), 0);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn duplicate_chunks_protect_all_references_and_aliases() {
    let c = Store::default();
    let a = Store::default();
    let mut store = Reclaiming::open_chunked(
        c.clone(),
        BTreeMap::from([("east".into(), a.clone()), ("west".into(), a.clone())]),
        |_| "east".into(),
        Default::default(),
        JournalChunkOptions {
            max_records: 1,
            ..selected()
        },
    )
    .unwrap();
    let keys = vec!["a".into()];
    let data = vec!["same".into(), "same".into()];
    let base = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &base, &[Some(data.clone())])
        .unwrap());
    assert_eq!(a.0.lock().unwrap().values.len(), 1);
    assert_eq!(store.collect_garbage().unwrap().unwrap().deleted, 0);
    assert_eq!(store.read(&keys).unwrap()[0].entries, data);
    c.0.lock().unwrap().values.get_mut("a").unwrap().entries[8] = "99".into();
    assert!(store.collect_garbage().is_err());
    assert_eq!(a.count("delete"), 0);
}
