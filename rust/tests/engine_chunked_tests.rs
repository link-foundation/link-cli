use link_cli::engine::{
    cluster::{AtomicJournalStore, JournalVersion},
    partitioned::{JournalChunkOptions, PartitionedJournalStore},
};
use link_cli::LinkError;
#[path = "engine_chunked/corruption.rs"]
mod corruption;
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
    reads: usize,
    writes: usize,
    bytes: usize,
    before: Option<usize>,
    after: Option<usize>,
    mutations: usize,
}
impl AtomicJournalStore for Store {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        let mut state = self.0.lock().unwrap();
        state.reads += 1;
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
        let mutation = state.mutations;
        state.mutations += 1;
        if state.before == Some(mutation) {
            return Err(LinkError::Transaction("finite publication cut".into()));
        }
        if keys.iter().zip(expected).any(|(key, value)| {
            state.values.get(key).map_or(0, |v| v.generation) != value.generation
        }) {
            return Ok(false);
        }
        state.revision += 1;
        let generation = state.revision;
        for (key, update) in keys.iter().zip(updates) {
            if let Some(entries) = update {
                state.bytes += entries.iter().map(String::len).sum::<usize>();
                state.values.insert(
                    key.clone(),
                    JournalVersion {
                        generation,
                        entries: entries.clone(),
                    },
                );
            }
        }
        state.writes += 1;
        if state.after == Some(mutation) {
            return Err(LinkError::Transaction("finite publication cut".into()));
        }
        Ok(true)
    }
}
fn fixture(options: JournalChunkOptions) -> (Store, Store, PartitionedJournalStore<Store, Store>) {
    let catalog = Store::default();
    let data = Store::default();
    let store = PartitionedJournalStore::new(
        catalog.clone(),
        BTreeMap::from([("east".into(), data.clone())]),
        |_| "east".into(),
    )
    .unwrap()
    .with_chunks(options)
    .unwrap();
    (catalog, data, store)
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn shared_independent_chunk_vectors_preserve_utf8_boundaries_and_live_empty() {
    let vectors: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../docs/protocol/chunked-journal-vectors.json"
    ))
    .unwrap();
    for v in vectors {
        let (catalog, data, mut store) = fixture(JournalChunkOptions {
            max_records: v["maxRecords"].as_u64().unwrap() as usize,
            max_bytes: v["maxBytes"].as_u64().unwrap() as usize,
            max_chunks: 32,
        });
        let keys = vec!["a".into()];
        let entries: Vec<String> = serde_json::from_value(v["entries"].clone()).unwrap();
        let base = store.read(&keys).unwrap();
        assert!(store
            .compare_exchange(&keys, &base, &[Some(entries.clone())])
            .unwrap());
        let chunks = v["chunks"].as_array().unwrap();
        let mut manifest = vec![
            "chunks1".into(),
            "east".into(),
            entries.len().to_string(),
            chunks.len().to_string(),
        ];
        for (index, chunk) in chunks.iter().enumerate() {
            manifest.extend([
                chunk["address"].as_str().unwrap().to_owned(),
                (index + 1).to_string(),
                chunk["entries"].as_array().unwrap().len().to_string(),
            ]);
        }
        assert_eq!(catalog.0.lock().unwrap().values["a"].entries, manifest);
        assert_eq!(data.0.lock().unwrap().values.len(), chunks.len());
        assert_eq!(store.read(&keys).unwrap()[0].entries, entries);
        let mut legacy =
            PartitionedJournalStore::new(catalog, BTreeMap::from([("east".into(), data)]), |_| {
                "east".into()
            })
            .unwrap();
        assert_eq!(legacy.read(&keys).unwrap()[0].entries, entries);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn append_reuses_prefixes_but_staging_does_not_trust_read_cache() {
    let (catalog, data, store) = fixture(JournalChunkOptions {
        max_records: 2,
        max_bytes: 8,
        max_chunks: 32,
    });
    let mut store = store.with_object_cache(256);
    let keys = vec!["a".into()];
    let seed: Vec<String> = ["aa", "bb", "cc", "dd"]
        .into_iter()
        .map(String::from)
        .collect();
    let base = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &base, &[Some(seed.clone())])
        .unwrap());
    let pointer = catalog.0.lock().unwrap().values["a"].clone();
    let owned = store.read(&keys).unwrap();
    let reads = data.0.lock().unwrap().reads;
    store.read(&keys).unwrap();
    assert_eq!(reads, data.0.lock().unwrap().reads);
    let bytes = data.0.lock().unwrap().bytes;
    let writes = data.0.lock().unwrap().writes;
    let mut extended = seed.clone();
    extended.push("ee".into());
    assert!(store
        .compare_exchange(&keys, &owned, &[Some(extended.clone())])
        .unwrap());
    assert_eq!(data.0.lock().unwrap().bytes - bytes, 2);
    assert_eq!(data.0.lock().unwrap().writes - writes, 1);
    assert_eq!(owned[0].entries, seed);
    let mut returned = store.read(&keys).unwrap();
    returned[0].entries[0] = "caller".into();
    assert_eq!(store.read(&keys).unwrap()[0].entries, extended);
    catalog
        .0
        .lock()
        .unwrap()
        .values
        .insert("a".into(), pointer.clone());
    assert_eq!(store.read(&keys).unwrap()[0].entries, seed);
    data.0
        .lock()
        .unwrap()
        .values
        .get_mut(&pointer.entries[4])
        .unwrap()
        .entries = vec!["collision".into()];
    assert!(store
        .compare_exchange(&keys, &owned, &[Some(seed)])
        .is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_chunk_policies_and_complete_updates_fail_before_storage_access() {
    for options in [
        JournalChunkOptions {
            max_records: 0,
            max_bytes: 1,
            max_chunks: 1,
        },
        JournalChunkOptions {
            max_records: 65537,
            max_bytes: 1,
            max_chunks: 1,
        },
        JournalChunkOptions {
            max_records: 1,
            max_bytes: 0,
            max_chunks: 1,
        },
        JournalChunkOptions {
            max_records: 1,
            max_bytes: 8388609,
            max_chunks: 1,
        },
        JournalChunkOptions {
            max_records: 1,
            max_bytes: 1,
            max_chunks: 0,
        },
        JournalChunkOptions {
            max_records: 1,
            max_bytes: 1,
            max_chunks: 4097,
        },
    ] {
        let catalog = Store::default();
        let data = Store::default();
        assert!(PartitionedJournalStore::new(
            catalog.clone(),
            BTreeMap::from([("east".into(), data.clone())]),
            |_| "east".into()
        )
        .unwrap()
        .with_chunks(options)
        .is_err());
        assert_eq!(
            catalog.0.lock().unwrap().reads + data.0.lock().unwrap().reads,
            0
        );
    }
    let (catalog, data, mut store) = fixture(JournalChunkOptions {
        max_records: 1,
        max_bytes: 4,
        max_chunks: 2,
    });
    let keys = vec!["a".into(), "b".into()];
    let expected = vec![JournalVersion::default(); 2];
    for records in [
        vec!["12345".into()],
        vec!["a".into(), "b".into(), "c".into()],
    ] {
        assert!(store
            .compare_exchange(&keys, &expected, &[Some(vec!["one".into()]), Some(records)])
            .is_err());
        let accesses = {
            let state = data.0.lock().unwrap();
            state.reads + state.writes
        };
        assert_eq!(catalog.0.lock().unwrap().reads + accesses, 0);
    }
    let mut bad = PartitionedJournalStore::new(
        catalog,
        BTreeMap::from([("east".into(), data.clone())]),
        |key| {
            if key == "a" {
                "east".into()
            } else {
                "missing".into()
            }
        },
    )
    .unwrap()
    .with_chunks(JournalChunkOptions::default())
    .unwrap();
    assert!(bad
        .compare_exchange(
            &keys,
            &expected,
            &[Some(vec!["one".into()]), Some(vec!["two".into()])]
        )
        .is_err());
    assert_eq!(data.0.lock().unwrap().reads, 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn all_selected_manifests_validate_before_any_data_fetch() {
    let (catalog, data, mut store) = fixture(JournalChunkOptions::default());
    let keys = vec!["a".into()];
    let base = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &base, &[Some(vec!["one".into()])])
        .unwrap());
    let pointer = catalog.0.lock().unwrap().values["a"].clone();
    let key = &pointer.entries[4];
    let generation = &pointer.entries[5];
    let malformed: Vec<Vec<String>> = vec![
        vec!["chunks1"],
        vec!["chunks1", "missing", "0", "0"],
        vec!["chunks1", "east", "00", "0"],
        vec!["chunks1", "east", "4294967296", "0"],
        vec!["chunks1", "east", "0", "4097"],
        vec!["chunks1", "east", "0", "01"],
        vec!["chunks1", "east", "0", "1"],
        vec!["chunks1", "east", "1", "0"],
        vec!["chunks1", "east", "1", "1", "../bad", generation, "1"],
        vec!["chunks1", "east", "1", "1", key, "0", "1"],
        vec!["chunks1", "east", "1", "1", key, "01", "1"],
        vec![
            "chunks1",
            "east",
            "1",
            "1",
            key,
            "18446744073709551616",
            "1",
        ],
        vec!["chunks1", "east", "1", "1", key, generation, "0"],
        vec!["chunks1", "east", "1", "1", key, generation, "65537"],
        vec!["chunks1", "east", "2", "1", key, generation, "1"],
    ]
    .into_iter()
    .map(|v| v.into_iter().map(String::from).collect())
    .collect();
    for entries in malformed {
        catalog.0.lock().unwrap().values.insert(
            "b".into(),
            JournalVersion {
                generation: 1,
                entries,
            },
        );
        let reads = data.0.lock().unwrap().reads;
        assert!(store.read(&["a".into(), "b".into()]).is_err());
        assert_eq!(reads, data.0.lock().unwrap().reads);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_finite_publication_cut_exposes_only_complete_groups_and_following_edits() {
    for cut in 0..5 {
        for after in [false, true] {
            let (catalog, data, mut store) = fixture(JournalChunkOptions {
                max_records: 2,
                max_bytes: 8,
                max_chunks: 32,
            });
            let keys = vec!["a".into(), "b".into()];
            let old = vec![Some(vec!["old".into()]), Some(vec!["old-b".into()])];
            let base = store.read(&keys).unwrap();
            assert!(store.compare_exchange(&keys, &base, &old).unwrap());
            let baseline = store.read(&keys).unwrap();
            // Four independent staged object mutations, then one catalog mutation.
            // The two mock stores have separate counters, so target their exact side.
            {
                let mut d = data.0.lock().unwrap();
                let mut c = catalog.0.lock().unwrap();
                d.mutations = 0;
                c.mutations = 0;
                if cut < 4 {
                    if after {
                        d.after = Some(cut);
                    } else {
                        d.before = Some(cut);
                    }
                } else if after {
                    c.after = Some(0);
                } else {
                    c.before = Some(0);
                }
            }
            let updates = vec![
                Some(vec!["a".into(), "b".into(), "c".into()]),
                Some(vec!["d".into(), "e".into(), "f".into()]),
            ];
            assert!(store.compare_exchange(&keys, &baseline, &updates).is_err());
            {
                let mut d = data.0.lock().unwrap();
                d.before = None;
                d.after = None;
            }
            {
                let mut c = catalog.0.lock().unwrap();
                c.before = None;
                c.after = None;
            }
            let fresh = store.read(&keys).unwrap();
            let selected = if after && cut == 4 { &updates } else { &old };
            assert_eq!(&fresh[0].entries, selected[0].as_ref().unwrap());
            assert_eq!(&fresh[1].entries, selected[1].as_ref().unwrap());
            assert!(store
                .compare_exchange(&keys, &fresh, &[Some(vec!["next".into()]), None])
                .unwrap());
            assert!(!store
                .compare_exchange(&keys, &baseline, &[Some(vec!["stale".into()]), None])
                .unwrap());
        }
    }
}
