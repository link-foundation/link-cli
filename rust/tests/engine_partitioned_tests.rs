use link_cli::engine::{
    cluster::{AtomicJournalStore, JournalVersion},
    partitioned::PartitionedJournalStore,
};
use link_cli::LinkError;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct Store(Arc<Mutex<State>>);
#[derive(Default)]
struct State {
    values: BTreeMap<String, JournalVersion>,
    revision: u64,
    fail: bool,
    lose_reply: bool,
    volatile: bool,
    short_read: bool,
    race: Option<&'static str>,
}
impl AtomicJournalStore for Store {
    fn supports_durable_flush(&self) -> bool {
        !self.0.lock().unwrap().volatile
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        let state = self.0.lock().unwrap();
        if state.fail {
            return Err(LinkError::Transaction("partition unavailable".into()));
        }
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
        if state.fail {
            return Err(LinkError::Transaction("partition unavailable".into()));
        }
        if let Some(race) = state.race {
            if race != "absent" {
                state.values.insert(
                    keys[0].clone(),
                    JournalVersion {
                        generation: 1,
                        entries: match race {
                            "identical" => updates[0].clone().unwrap(),
                            "different-content" => vec!["collision".into()],
                            _ => vec![],
                        },
                    },
                );
            }
            return Ok(false);
        }
        if keys
            .iter()
            .zip(expected)
            .any(|(key, item)| state.values.get(key).map_or(0, |v| v.generation) != item.generation)
        {
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
            return Err(LinkError::Transaction("reply lost".into()));
        }
        Ok(true)
    }
}
fn fixture() -> (Store, Store, Store, PartitionedJournalStore<Store, Store>) {
    let catalog = Store::default();
    let a = Store::default();
    let b = Store::default();
    let store = PartitionedJournalStore::new(
        catalog.clone(),
        BTreeMap::from([("east".into(), a.clone()), ("west".into(), b.clone())]),
        |key| {
            if key == "a" {
                "east".into()
            } else {
                "west".into()
            }
        },
    )
    .unwrap();
    (catalog, a, b, store)
}
fn keys() -> Vec<String> {
    vec!["a".into(), "b".into()]
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn portable_addresses_frame_utf8_bytes_and_entry_boundaries() {
    let vectors: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../docs/protocol/partitioned-journal-vectors.json"
    ))
    .unwrap();
    for vector in vectors {
        let (_, a, _, mut store) = fixture();
        let keys = vec!["a".into()];
        let entries: Vec<String> = serde_json::from_value(vector["entries"].clone()).unwrap();
        let snapshot = store.read(&keys).unwrap();
        assert!(store
            .compare_exchange(&keys, &snapshot, &[Some(entries.clone())])
            .unwrap());
        assert_eq!(
            a.0.lock().unwrap().values.keys().next().unwrap(),
            vector["address"].as_str().unwrap()
        );
        assert_eq!(store.read(&keys).unwrap()[0].entries, entries);
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_objects_are_published_by_atomic_small_pointers() {
    let (catalog, a, b, mut store) = fixture();
    let keys = keys();
    let base = store.read(&keys).unwrap();
    assert!(store.supports_durable_flush());
    let data = vec![Some(vec!["journal-é".into()]), Some(vec!["second".into()])];
    assert!(store.compare_exchange(&keys, &base, &data).unwrap());
    assert_eq!(a.0.lock().unwrap().values.len(), 1);
    assert_eq!(b.0.lock().unwrap().values.len(), 1);
    assert!(!format!("{:?}", catalog.0.lock().unwrap().values).contains("journal-é"));
    let stale = store.read(&keys).unwrap();
    assert_eq!(stale[0].entries, ["journal-é"]);
    assert!(store
        .compare_exchange(&keys[1..], &stale[1..], &[Some(vec!["changed".into()])])
        .unwrap());
    assert!(!store
        .compare_exchange(&keys, &stale, &[Some(vec!["abandoned".into()]), None])
        .unwrap());
    assert_eq!(store.read(&keys).unwrap()[0].entries, ["journal-é"]);
    let count = a.0.lock().unwrap().values.len();
    let fresh = store.read(&keys[..1]).unwrap();
    assert!(store
        .compare_exchange(&keys[..1], &fresh, &[Some(vec!["abandoned".into()])])
        .unwrap());
    assert_eq!(a.0.lock().unwrap().values.len(), count);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn failures_before_publication_leave_only_orphans_and_lost_reply_is_discovered() {
    let (catalog, _, b, mut store) = fixture();
    let keys = keys();
    let base = store.read(&keys).unwrap();
    let updates = vec![Some(vec!["one".into()]), Some(vec!["two".into()])];
    b.0.lock().unwrap().fail = true;
    assert!(store.compare_exchange(&keys, &base, &updates).is_err());
    assert!(catalog.0.lock().unwrap().values.is_empty());
    b.0.lock().unwrap().fail = false;
    catalog.0.lock().unwrap().lose_reply = true;
    assert!(store.compare_exchange(&keys, &base, &updates).is_err());
    let read = store.read(&keys).unwrap();
    assert_eq!(read[0].entries, ["one"]);
    assert_eq!(read[1].entries, ["two"]);
    b.0.lock().unwrap().fail = true;
    assert!(store.read(&keys).is_err());
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn corrupt_or_missing_objects_and_invalid_coordinates_fail_closed() {
    let (catalog, a, _, mut store) = fixture();
    let keys = vec!["a".into()];
    let base = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &base, &[Some(vec!["one".into()])])
        .unwrap());
    let (key, blob) =
        a.0.lock()
            .unwrap()
            .values
            .first_key_value()
            .map(|(k, v)| (k.clone(), v.clone()))
            .unwrap();
    a.0.lock().unwrap().values.clear();
    assert!(store.read(&keys).is_err());
    a.0.lock().unwrap().values.insert(
        key.clone(),
        JournalVersion {
            generation: 99,
            entries: blob.entries.clone(),
        },
    );
    assert!(store.read(&keys).is_err());
    a.0.lock().unwrap().values.insert(
        key.clone(),
        JournalVersion {
            generation: blob.generation,
            entries: vec!["different".into()],
        },
    );
    assert!(store.read(&keys).is_err());
    for parts in [
        vec![],
        vec!["other".into(), "east".into(), key.clone(), "1".into()],
        vec!["blob1".into(), "absent".into(), key.clone(), "1".into()],
        vec!["blob1".into(), "east".into(), key.clone(), "01".into()],
    ] {
        catalog
            .0
            .lock()
            .unwrap()
            .values
            .get_mut("a")
            .unwrap()
            .entries = parts;
        assert!(store.read(&keys).is_err());
    }
    assert!(store.read(&[]).is_err());
    assert!(store.compare_exchange(&keys, &[], &[None]).is_err());
    assert!(PartitionedJournalStore::new(
        Store::default(),
        BTreeMap::<String, Store>::new(),
        |_| "east".into()
    )
    .is_err());
    let mut wrong = PartitionedJournalStore::new(
        Store::default(),
        BTreeMap::from([("east".into(), Store::default())]),
        |_| "absent".into(),
    )
    .unwrap();
    assert!(wrong
        .compare_exchange(&keys, &[JournalVersion::default()], &[Some(vec![])])
        .is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn durability_and_every_route_are_validated_before_staging() {
    let (catalog, a, b, store) = fixture();
    catalog.0.lock().unwrap().volatile = true;
    assert!(!store.supports_durable_flush());
    catalog.0.lock().unwrap().volatile = false;
    b.0.lock().unwrap().volatile = true;
    assert!(!store.supports_durable_flush());
    b.0.lock().unwrap().volatile = false;
    assert!(store.supports_durable_flush());
    assert!(PartitionedJournalStore::new(
        catalog.clone(),
        BTreeMap::from([("../east".into(), a.clone())]),
        |_| "east".into()
    )
    .is_err());
    let mut wrong = PartitionedJournalStore::new(
        catalog.clone(),
        BTreeMap::from([("east".into(), a.clone())]),
        |key| {
            if key == "a" {
                "east".into()
            } else {
                "missing".into()
            }
        },
    )
    .unwrap();
    assert!(wrong
        .compare_exchange(
            &keys(),
            &[JournalVersion::default(), JournalVersion::default()],
            &[Some(vec!["one".into()]), Some(vec!["two".into()])]
        )
        .is_err());
    assert!(catalog.0.lock().unwrap().values.is_empty());
    assert!(a.0.lock().unwrap().values.is_empty());
    assert!(wrong
        .compare_exchange(
            &keys(),
            &[JournalVersion::default(), JournalVersion::default()],
            &[None]
        )
        .is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn malformed_snapshot_arity_and_pointer_coordinates_fail_closed() {
    let (catalog, a, _, mut store) = fixture();
    let keys = vec!["a".into()];
    let base = store.read(&keys).unwrap();
    store
        .compare_exchange(&keys, &base, &[Some(vec!["original".into()])])
        .unwrap();
    let pointer = catalog.0.lock().unwrap().values["a"].clone();
    for generation in ["", "0", "-1", "+1", "1 ", "18446744073709551616", "01"] {
        let mut item = pointer.clone();
        item.entries[3] = generation.into();
        catalog.0.lock().unwrap().values.insert("a".into(), item);
        assert!(
            store.read(&keys).is_err(),
            "accepted generation {generation:?}"
        );
    }
    for address in [
        "../bad",
        "other-0000000000000000",
        "blob1-000000000000000G",
        "blob1-0000000000000000\n",
    ] {
        let mut item = pointer.clone();
        item.entries[2] = address.into();
        catalog.0.lock().unwrap().values.insert("a".into(), item);
        assert!(store.read(&keys).is_err(), "accepted address {address:?}");
    }
    let mut absent = pointer.clone();
    absent.generation = 0;
    catalog.0.lock().unwrap().values.insert("a".into(), absent);
    assert!(store.read(&keys).is_err());
    catalog
        .0
        .lock()
        .unwrap()
        .values
        .insert("a".into(), pointer.clone());
    catalog.0.lock().unwrap().short_read = true;
    assert!(store.read(&keys).is_err());
    catalog.0.lock().unwrap().short_read = false;
    a.0.lock().unwrap().short_read = true;
    assert!(store.read(&keys).is_err());
    assert!(store
        .compare_exchange(&keys, &[pointer], &[Some(vec!["other".into()])])
        .is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unavailable_object_staging_never_publishes_a_catalog_pointer() {
    let (catalog, a, _, mut store) = fixture();
    let keys = vec!["a".into()];
    let expected = store.read(&keys).unwrap();
    a.0.lock().unwrap().fail = true;
    assert!(store
        .compare_exchange(&keys, &expected, &[Some(vec!["new".into()])])
        .is_err());
    assert!(catalog.0.lock().unwrap().values.is_empty());
    assert!(a.0.lock().unwrap().values.is_empty());
    a.0.lock().unwrap().fail = false;
    assert!(store
        .compare_exchange(&keys, &expected, &[Some(vec!["new".into()])])
        .unwrap());
    assert_eq!(store.read(&keys).unwrap()[0].entries, ["new"]);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn concurrent_object_creation_reuses_only_the_complete_same_journal() {
    for competing in [
        "identical",
        "different-content",
        "different-length",
        "absent",
    ] {
        let (catalog, a, _, mut store) = fixture();
        let keys = vec!["a".into()];
        let snapshot = store.read(&keys).unwrap();
        a.0.lock().unwrap().race = Some(competing);
        let result = store.compare_exchange(&keys, &snapshot, &[Some(vec!["original".into()])]);
        if competing == "identical" {
            assert!(result.unwrap());
            assert_eq!(store.read(&keys).unwrap()[0].entries, ["original"]);
        } else {
            assert!(result.is_err());
            assert!(catalog.0.lock().unwrap().values.is_empty());
        }
    }
}
