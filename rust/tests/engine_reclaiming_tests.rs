use link_cli::{
    engine::{
        cluster::{AtomicJournalStore, JournalVersion},
        maintenance::{JournalMaintenanceStore, JournalNamespaceEntry, JournalScanOptions},
        reclaiming::{ReclaimingPartitionedJournalStore, ReclamationOptions},
    },
    LinkError,
};
use std::collections::BTreeMap;
fn error(text: &str) -> LinkError {
    LinkError::Transaction(text.into())
}
#[path = "engine_reclaiming/memory.rs"]
mod memory;
use memory::Store;
type Reclaiming = ReclaimingPartitionedJournalStore<Store, Store>;
const EPOCH: &str = "__link_gc_epoch";
fn open(c: &Store, a: &Store, b: &Store, options: ReclamationOptions) -> Reclaiming {
    Reclaiming::open(
        c.clone(),
        BTreeMap::from([("east".into(), a.clone()), ("west".into(), b.clone())]),
        |key| {
            if key == "a" {
                "east".into()
            } else {
                "west".into()
            }
        },
        options,
    )
    .unwrap()
}
fn fixture() -> (Store, Store, Store, Reclaiming) {
    let c = Store::default();
    let a = Store::default();
    let b = Store::default();
    let store = open(&c, &a, &b, Default::default());
    (c, a, b, store)
}
fn keys() -> Vec<String> {
    vec!["a".into(), "b".into()]
}
fn write(store: &mut Reclaiming, a: &str, b: &str) -> Vec<JournalVersion> {
    let prior = store.read(&keys()).unwrap();
    assert!(store
        .compare_exchange(
            &keys(),
            &prior,
            &[Some(vec![a.into()]), Some(vec![b.into()])]
        )
        .unwrap());
    store.read(&keys()).unwrap()
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn explicit_collection_preserves_current_roots_owned_reads_and_stale_guards() {
    let (c, a, b, mut store) = fixture();
    assert!(!store.supports_durable_flush());
    assert_eq!(c.0.lock().unwrap().values[EPOCH].entries, ["gc1", "0"]);
    assert_eq!(
        store.read(&keys()).unwrap(),
        vec![JournalVersion::default(); 2]
    );
    let initial = write(&mut store, "a0 雪", "b0");
    write(&mut store, "a1", "b1");
    assert_eq!(a.0.lock().unwrap().values.len(), 2);
    assert_eq!(b.0.lock().unwrap().values.len(), 2);
    assert_eq!(a.count("delete"), 0);
    let result = store.collect_garbage().unwrap().unwrap();
    assert_eq!((result.epoch, result.examined, result.deleted), (1, 4, 2));
    assert_eq!(a.0.lock().unwrap().values.len(), 1);
    assert_eq!(b.0.lock().unwrap().values.len(), 1);
    assert_eq!(store.read(&keys()).unwrap()[0].entries, ["a1"]);
    assert_eq!(initial[0].entries, ["a0 雪"]);
    assert!(!store
        .compare_exchange(&keys(), &initial, &[Some(vec!["stale".into()]), None])
        .unwrap());
    assert!(store.read(&[EPOCH.into()]).is_err());
    assert!(store
        .read(&(0..32).map(|i| format!("s{i}")).collect::<Vec<_>>())
        .is_err());
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn epoch_guard_fences_a_publisher_after_staging() {
    let (c, a, b, mut writer) = fixture();
    let mut collector = open(&c, &a, &b, Default::default());
    write(&mut writer, "a0", "b0");
    let before = writer.read(&["a".into()]).unwrap();
    a.on("swap", true, move || {
        assert_eq!(collector.collect_garbage()?.unwrap().epoch, 1);
        Ok(())
    });
    assert!(!writer
        .compare_exchange(&["a".into()], &before, &[Some(vec!["in-flight".into()])])
        .unwrap_or(false));
    assert_eq!(writer.read(&keys()).unwrap()[0].entries, ["a0"]);
    let current = writer.read(&["a".into()]).unwrap();
    assert!(writer
        .compare_exchange(&["a".into()], &current, &[Some(vec!["following".into()])])
        .unwrap());
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn missing_retired_shard_refreshes_the_whole_group() {
    let (c, a, b, mut writer) = fixture();
    let mut reader = open(&c, &a, &b, Default::default());
    write(&mut writer, "a0", "b0");
    b.on("read", false, move || {
        write(&mut writer, "a1", "b1");
        assert_eq!(writer.collect_garbage()?.unwrap().deleted, 2);
        Ok(())
    });
    let group = reader.read(&keys()).unwrap();
    assert_eq!(group[0].entries, ["a1"]);
    assert_eq!(group[1].entries, ["b1"]);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn incomplete_scans_delete_nothing_and_unknown_deletion_is_not_replayed() {
    let (_, a, b, mut store) = fixture();
    write(&mut store, "a0", "b0");
    write(&mut store, "a1", "b1");
    b.on("scan", false, || Err(error("incomplete provider snapshot")));
    assert!(store.collect_garbage().is_err());
    assert_eq!(a.count("delete"), 0);
    assert_eq!(b.count("delete"), 0);
    a.on("delete", true, || Err(error("delete response lost")));
    assert!(store.collect_garbage().is_err());
    assert_eq!(a.count("delete"), 1);
    assert_eq!(a.0.lock().unwrap().values.len(), 1);
    assert_eq!(store.read(&keys()).unwrap()[0].entries, ["a1"]);
    assert_eq!(store.collect_garbage().unwrap().unwrap().deleted, 1);
}

#[path = "engine_reclaiming/vectors.rs"]
mod vectors;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn aliased_providers_preserve_every_live_root() {
    let c = Store::default();
    let a = Store::default();
    let mut store = open(&c, &a, &a, Default::default());
    let keys = vec!["a".into()];
    let expected = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &expected, &[Some(vec!["live".into()])])
        .unwrap());
    assert_eq!(store.collect_garbage().unwrap().unwrap().deleted, 0);
    assert_eq!(store.read(&keys).unwrap()[0].entries, ["live"]);
}

#[path = "engine_reclaiming/faults.rs"]
mod faults;

#[path = "engine_reclaiming/boundaries.rs"]
mod boundaries;

#[path = "engine_reclaiming/concurrent.rs"]
mod concurrent;

#[path = "engine_reclaiming/contracts.rs"]
mod contracts;
