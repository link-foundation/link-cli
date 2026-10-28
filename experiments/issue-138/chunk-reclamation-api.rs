// Original minimum control; the baseline driver supplies old source and helpers.
use super::*;
use link_cli::engine::partitioned::JournalChunkOptions;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn selected_chunks_reuse_within_epoch_and_restage_after_fence() {
    let c = Store::default();
    let a = Store::default();
    let mut store = Reclaiming::open_chunked(
        c.clone(),
        BTreeMap::from([("east".into(), a.clone())]),
        |_| "east".into(),
        ReclamationOptions { cache_bytes: 256, ..Default::default() },
        JournalChunkOptions { max_records: 2, max_bytes: 64, max_chunks: 32 },
    ).unwrap();
    let keys = vec!["a".into()];
    let seed: Vec<String> = ["aa", "bb", "cc", "dd"].into_iter().map(String::from).collect();
    let base = store.read(&keys).unwrap();
    assert!(store.compare_exchange(&keys, &base, &[Some(seed.clone())]).unwrap());
    let original = c.0.lock().unwrap().values["a"].clone();
    assert_eq!(original.entries[0], "chunks2");
    assert_eq!(original.entries[3], "2");
    let extended: Vec<_> = seed.iter().cloned().chain(["ee".into()]).collect();
    let base = store.read(&keys).unwrap();
    assert!(store.compare_exchange(&keys, &base, &[Some(extended.clone())]).unwrap());
    let appended = c.0.lock().unwrap().values["a"].clone();
    assert_eq!(appended.entries[4..10], original.entries[4..10]);
    assert_eq!(a.count("swap"), 3);
    let owned = store.read(&keys).unwrap();
    assert_eq!(store.collect_garbage().unwrap().unwrap().epoch, 1);
    let fresh: Vec<_> = extended.iter().cloned().chain(["ff".into()]).collect();
    assert!(store.compare_exchange(&keys, &owned, &[Some(fresh.clone())]).unwrap());
    let root = c.0.lock().unwrap().values["a"].clone();
    for triple in root.entries[4..].as_chunks::<3>().0 {
        assert!(triple[0].starts_with("blob2-0000000000000001-"));
    }
    assert_eq!(owned[0].entries, extended);
    assert_eq!(store.collect_garbage().unwrap().unwrap().deleted, 3);
    assert_eq!(store.read(&keys).unwrap()[0].entries, fresh);
}
