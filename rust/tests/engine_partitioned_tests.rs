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
}
impl AtomicJournalStore for Store {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        let state = self.0.lock().unwrap();
        if state.fail {
            return Err(LinkError::Transaction("partition unavailable".into()));
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
#[test]
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
#[test]
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
#[test]
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
#[test]
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
