#![cfg(feature = "causal-batches")]
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_batch::{CausalBatch, CausalBatchEdit},
    causal_replica::{CausalReplica, CausalReplicaOptions},
    cluster::{AtomicJournalStore, JournalVersion},
};
use link_cli::LinkError;
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

const A: &str = "00000000000000000000000000000001";
const B: &str = "00000000000000000000000000000002";
#[derive(Default)]
struct Data {
    values: BTreeMap<String, JournalVersion>,
    calls: usize,
    failure: u8,
    before: Option<Box<dyn FnOnce()>>,
    corrupt: Option<Vec<JournalVersion>>,
}
#[derive(Clone, Default)]
struct Store(Rc<RefCell<Data>>);
impl AtomicJournalStore for Store {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        let data = self.0.borrow();
        Ok(data.corrupt.clone().unwrap_or_else(|| {
            keys.iter()
                .map(|key| data.values.get(key).cloned().unwrap_or_default())
                .collect()
        }))
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        let before = self.0.borrow_mut().before.take();
        if let Some(before) = before {
            before();
        }
        let mut data = self.0.borrow_mut();
        data.calls += 1;
        let failure = std::mem::take(&mut data.failure);
        if failure == 1 {
            return Err(LinkError::StorageError("before".into()));
        }
        if keys.iter().zip(expected).any(|(key, value)| {
            data.values.get(key).map_or(0, |v| v.generation) != value.generation
        }) {
            return Ok(false);
        }
        for ((key, value), update) in keys.iter().zip(expected).zip(updates) {
            data.values.insert(
                key.clone(),
                JournalVersion {
                    generation: value.generation + 1,
                    entries: update.clone().unwrap(),
                },
            );
        }
        if failure == 2 {
            return Err(LinkError::StorageError("after".into()));
        }
        Ok(true)
    }
}
fn batch(store: Store) -> CausalBatch<Store> {
    CausalBatch::new(
        store,
        "replica",
        "batches",
        A,
        CausalReplicaOptions::default(),
    )
    .unwrap()
}
fn owner(store: Store) -> CausalReplica<Store> {
    CausalReplica::new(store, "replica", A, CausalReplicaOptions::default()).unwrap()
}
fn edits() -> Vec<CausalBatchEdit> {
    vec![
        CausalBatchEdit {
            address: 1,
            observed: vec![],
            payload: Some(CausalPayload {
                source: 0,
                target: 2,
                name: Some("\u{feff}雪😀\0".into()),
            }),
        },
        CausalBatchEdit {
            address: 2,
            observed: vec![],
            payload: None,
        },
    ]
}
fn token(number: u128) -> String {
    format!("{number:032x}")
}
fn stored(store: &Store) -> BTreeMap<String, JournalVersion> {
    store.0.borrow().values.clone()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_batch_journal_vectors() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../docs/protocol/causal-batch-v1.json")).unwrap();
    for fixture in fixtures["cases"].as_array().unwrap() {
        let store = Store::default();
        for (key, field) in [("replica", "owner"), ("batches", "receipts")] {
            store.0.borrow_mut().values.insert(
                key.into(),
                JournalVersion {
                    generation: fixture[field]["generation"]
                        .as_str()
                        .unwrap()
                        .parse()
                        .unwrap(),
                    entries: fixture[field]["entries"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().into())
                        .collect(),
                },
            );
        }
        let mut options = CausalReplicaOptions::default();
        if let Some(value) = fixture["options"]["maxEntries"].as_u64() {
            options.max_entries = value as usize;
        }
        if let Some(value) = fixture["options"]["maxBytes"].as_u64() {
            options.max_bytes = value as usize;
        }
        let before = stored(&store);
        let result = CausalBatch::new(store.clone(), "replica", "batches", A, options)
            .unwrap()
            .snapshot();
        if fixture["valid"].as_bool().unwrap() {
            let result = result.unwrap_or_else(|error| panic!("{}: {error}", fixture["name"]));
            assert_eq!(
                result.generation.to_string(),
                fixture["owner"]["generation"]
            );
            assert_eq!(result.last_counter.to_string(), fixture["lastCounter"]);
            let encoded: Vec<_> = result
                .versions
                .iter()
                .map(|v| serde_json::from_str::<Value>(&v.encode().unwrap()).unwrap())
                .collect();
            assert_eq!(
                encoded,
                *fixture["versions"].as_array().unwrap(),
                "{}",
                fixture["name"]
            );
        } else {
            assert!(result.is_err(), "{}", fixture["name"]);
        }
        assert_eq!(stored(&store), before);
        assert_eq!(store.0.borrow().calls, 0);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn whole_batch_publication_and_exact_retry_after_following_writes() {
    for failure in [0, 1, 2] {
        let store = Store::default();
        store.0.borrow_mut().failure = failure;
        let initial = batch(store.clone()).write(&token(100), 0, &edits());
        assert_eq!(initial.is_err(), failure != 0);
        assert_eq!(store.0.borrow().calls, 1);
        if failure == 1 {
            assert!(stored(&store).is_empty());
        }
        let result = batch(store.clone())
            .write(&token(100), 0, &edits())
            .unwrap();
        assert_eq!(
            result.iter().map(|v| v.dot.counter).collect::<Vec<_>>(),
            [1, 2]
        );
        let mut following = edits().remove(0);
        following.address = 3;
        let third = owner(store.clone())
            .write(&token(300), 3, &[], following.payload)
            .unwrap();
        assert_eq!(third.dot.counter, 3);
        let before = stored(&store);
        let calls = store.0.borrow().calls;
        assert_eq!(
            batch(store.clone())
                .write(&token(100), 0, &edits())
                .unwrap(),
            result
        );
        assert_eq!(stored(&store), before);
        assert_eq!(store.0.borrow().calls, calls);
        let mut changed = edits();
        changed[1].payload = changed[0].payload.clone();
        assert!(batch(store.clone())
            .write(&token(100), 0, &changed)
            .is_err());
        assert!(batch(store.clone())
            .write(&token(100), 2, &edits())
            .is_err());
        assert_eq!(stored(&store), before);
        assert_eq!(store.0.borrow().calls, calls);
        let last = batch(store.clone())
            .write(
                &token(400),
                batch(store.clone()).snapshot().unwrap().generation,
                &[],
            )
            .unwrap();
        assert!(last.is_empty());
        assert_eq!(batch(store).snapshot().unwrap().last_counter, 3);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn later_contradiction_child_collision_and_stale_base_publish_nothing() {
    let store = Store::default();
    let remote = CausalVersion::advance(2, B, 0, &[], None).unwrap();
    owner(store.clone())
        .receive(std::slice::from_ref(&remote))
        .unwrap();
    let mut wrong = remote;
    wrong.payload = edits()[0].payload.clone();
    let mut request = edits();
    request[1].observed.push(wrong);
    let generation = batch(store.clone()).snapshot().unwrap().generation;
    let before = stored(&store);
    let calls = store.0.borrow().calls;
    assert!(batch(store.clone())
        .write(&token(100), generation, &request)
        .is_err());
    assert!(batch(store.clone())
        .write(&token(100), 0, &edits())
        .is_err());
    assert_eq!(stored(&store), before);
    assert_eq!(store.0.borrow().calls, calls);
    let first = edits().remove(0);
    owner(store.clone())
        .write(&token(100), first.address, &first.observed, first.payload)
        .unwrap();
    let generation = batch(store.clone()).snapshot().unwrap().generation;
    let before = stored(&store);
    let calls = store.0.borrow().calls;
    assert!(batch(store.clone())
        .write(&token(100), generation, &edits())
        .is_err());
    assert_eq!(stored(&store), before);
    assert_eq!(store.0.borrow().calls, calls);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn conflicting_publication_and_finite_request_limits() {
    let store = Store::default();
    let other = store.clone();
    store.0.borrow_mut().before = Some(Box::new(move || {
        batch(other).write(&token(200), 0, &edits()).unwrap();
    }));
    assert!(batch(store.clone())
        .write(&token(100), 0, &edits())
        .unwrap_err()
        .to_string()
        .contains("conflict"));
    assert_eq!(batch(store.clone()).snapshot().unwrap().last_counter, 2);
    let before = stored(&store);
    let calls = store.0.borrow().calls;
    let mut excessive = edits()[0].clone();
    excessive.observed = vec![CausalVersion::advance(1, B, 0, &[], None).unwrap(); 65];
    for request in [
        vec![edits()[0].clone(); 65],
        vec![edits()[0].clone(); 2],
        vec![excessive],
    ] {
        assert!(batch(store.clone())
            .write(&token(300), 1, &request)
            .is_err());
    }
    assert!(batch(store.clone())
        .write(&token(u128::MAX), 1, &edits())
        .is_err());
    assert!(batch(store.clone()).write("invalid", 1, &[]).is_err());
    assert_eq!(stored(&store), before);
    assert_eq!(store.0.borrow().calls, calls);
    for key in ["", "replica", "bad/key"] {
        assert!(CausalBatch::new(
            store.clone(),
            "replica",
            key,
            A,
            CausalReplicaOptions::default()
        )
        .is_err());
    }
    store.0.borrow_mut().corrupt = Some(vec![]);
    assert!(batch(store).snapshot().is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn maximum_edits_empty_publication_and_capacity_exhaustion() {
    let store = Store::default();
    let request: Vec<_> = (1..=64)
        .map(|address| CausalBatchEdit {
            address,
            observed: vec![],
            payload: None,
        })
        .collect();
    let result = batch(store.clone())
        .write(&token(100), 0, &request)
        .unwrap();
    assert_eq!(result.len(), 64);
    assert_eq!(result[63].dot.counter, 64);
    assert_eq!(store.0.borrow().calls, 1);
    let limited = Store::default();
    let mut owner = CausalBatch::new(
        limited.clone(),
        "replica",
        "batches",
        A,
        CausalReplicaOptions {
            max_entries: 3,
            ..Default::default()
        },
    )
    .unwrap();
    owner.write(&token(100), 0, &edits()).unwrap();
    owner.write(&token(200), 1, &[]).unwrap();
    let before = stored(&limited);
    let calls = limited.0.borrow().calls;
    assert!(owner.write(&token(300), 2, &[]).is_err());
    assert_eq!(stored(&limited), before);
    assert_eq!(limited.0.borrow().calls, calls);
    let empty = Store::default();
    assert!(batch(empty.clone())
        .write(&token(100), 0, &[])
        .unwrap()
        .is_empty());
    let before = stored(&empty);
    let calls = empty.0.borrow().calls;
    assert!(batch(empty.clone())
        .write(&token(100), 0, &[])
        .unwrap()
        .is_empty());
    assert_eq!(stored(&empty), before);
    assert_eq!(empty.0.borrow().calls, calls);
    assert_eq!(batch(empty).snapshot().unwrap().last_counter, 0);
    let too_small = Store::default();
    assert!(CausalBatch::new(
        too_small.clone(),
        "replica",
        "batches",
        A,
        CausalReplicaOptions {
            max_entries: 2,
            ..Default::default()
        }
    )
    .unwrap()
    .write(&token(100), 0, &edits())
    .is_err());
    assert!(stored(&too_small).is_empty());
    assert_eq!(too_small.0.borrow().calls, 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn dominated_observation_equivocation_rejects_even_on_matching_retry() {
    let store = Store::default();
    let old = CausalVersion::advance(1, B, 0, &[], edits()[0].payload.clone()).unwrap();
    let newer = CausalVersion::advance(1, B, 1, std::slice::from_ref(&old), None).unwrap();
    let request = CausalBatchEdit {
        address: 1,
        observed: vec![old.clone(), newer.clone()],
        payload: None,
    };
    let first = batch(store.clone())
        .write(&token(100), 0, &[request])
        .unwrap();
    let mut retry = CausalBatchEdit {
        address: 1,
        observed: vec![newer.clone()],
        payload: None,
    };
    assert_eq!(
        batch(store.clone())
            .write(&token(100), 0, &[retry.clone()])
            .unwrap(),
        first
    );
    let before = stored(&store);
    let calls = store.0.borrow().calls;
    let mut forged = old;
    forged.payload = None;
    retry.observed = vec![forged, newer];
    assert!(batch(store.clone())
        .write(&token(100), 0, &[retry])
        .is_err());
    assert_eq!(stored(&store), before);
    assert_eq!(store.0.borrow().calls, calls);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn provider_frame_and_key_boundaries_reject_before_publication() {
    let store = Store::default();
    let mut selected = CausalBatch::new(
        store.clone(),
        "replica",
        "batches",
        A,
        CausalReplicaOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(selected.snapshot().unwrap().versions.is_empty());
    assert!(selected.write(&token(100), 0, &[]).unwrap().is_empty());
    for key in ["a".repeat(129), ".valid_-09Z/".into()] {
        assert!(CausalBatch::new(
            store.clone(),
            "replica",
            &key,
            A,
            CausalReplicaOptions::default()
        )
        .is_err());
    }
    let _owned = CausalBatch::new(
        store.clone(),
        "replica",
        ".valid_-09Z",
        A,
        CausalReplicaOptions::default(),
    )
    .unwrap()
    .into_store();
    let before = stored(&store);
    let calls = store.0.borrow().calls;
    let header = serde_json::json!(["causal-batches1", "replica", A]).to_string();
    let mut frames = vec![
        "]".into(),
        "[".into(),
        "\"".into(),
        "\\".into(),
        "\u{1f}".into(),
        "\u{7f}".into(),
        "}".into(),
    ];
    frames.push(format!("[{}]", vec!["null"; 32770].join(",")));
    frames.push("1".repeat(1_048_577));
    for frame in frames {
        store.0.borrow_mut().corrupt = Some(vec![
            before["replica"].clone(),
            JournalVersion {
                generation: 1,
                entries: vec![header.clone(), frame],
            },
        ]);
        assert!(selected.snapshot().is_err());
    }
    store.0.borrow_mut().corrupt = None;
    assert_eq!(stored(&store), before);
    assert_eq!(store.0.borrow().calls, calls);
}
