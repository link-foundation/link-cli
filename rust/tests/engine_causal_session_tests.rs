#![cfg(feature = "causal-sessions")]
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_batch::{CausalBatch, CausalBatchEdit},
    causal_projection::CausalConflictPolicy,
    causal_replica::{CausalReplica, CausalReplicaOptions},
    causal_session::{CausalSession, CausalWorkspaceOptions},
    cluster::{AtomicJournalStore, JournalVersion},
    Constraint, ConstraintValidation, StateStructure,
};
use link_cli::{LinkError, LinksStorage};
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
    [(1, 0, 2, Some("old")), (3, 1, 1, None), (4, 0, 2, None)]
        .into_iter()
        .map(|(address, source, target, name)| CausalBatchEdit {
            address,
            observed: vec![],
            payload: Some(CausalPayload {
                source,
                target,
                name: name.map(String::from),
            }),
        })
        .collect()
}
fn seeded() -> Store {
    let store = Store::default();
    batch(store.clone()).write(&token(10), 0, &edits()).unwrap();
    store
}
fn session(store: Store, first: u64, last: u64) -> CausalSession<Store> {
    CausalSession::new(
        batch(store),
        first,
        last,
        CausalConflictPolicy::MaxDot,
        CausalWorkspaceOptions::default(),
    )
    .unwrap()
}
fn token(number: u128) -> String {
    format!("{number:032x}")
}
fn stored(store: &Store) -> BTreeMap<String, JournalVersion> {
    store.0.borrow().values.clone()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn ordinary_links_names_and_observers_publish_one_owned_batch() {
    let store = seeded();
    let before = stored(&store);
    let calls = store.0.borrow().calls;
    let mut tx = session(store.clone(), 32, 33);
    assert_eq!(tx.get_name(1).as_deref(), Some("old"));
    let mut seen = Vec::new();
    assert_eq!(
        tx.create_link_observed(0, 2, &mut |a, b| seen.push((a, b)))
            .unwrap(),
        32
    );
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1.index, 32);
    tx.set_name(32, Some("\u{feff}雪😀\0".into())).unwrap();
    tx.update_link(1, 2, 0).unwrap();
    tx.delete_link(1).unwrap();
    // Raw deletion is non-cascading and duplicate pairs remain distinct.
    assert!(tx.get_link(3).is_some());
    assert!(tx.get_link(4).is_some());
    assert_eq!(tx.get_by_name("\u{feff}雪😀\0"), Some(32));
    assert_eq!(stored(&store), before);
    assert_eq!(store.0.borrow().calls, calls);
    let mut result = tx.commit(&token(100)).unwrap();
    assert_eq!(
        result.iter().map(|v| v.address).collect::<Vec<_>>(),
        [1, 32]
    );
    assert!(result[0].payload.is_none());
    assert_eq!(store.0.borrow().calls, calls + 1);
    result[1].payload.as_mut().unwrap().name = Some("changed returned result".into());
    assert_eq!(
        tx.commit(&token(100)).unwrap()[1]
            .payload
            .as_ref()
            .unwrap()
            .name
            .as_deref(),
        Some("\u{feff}雪😀\0")
    );
    assert!(tx.commit(&token(200)).is_err());
    assert!(tx.create_link(0, 0).is_err());
    let published = batch(store.clone()).snapshot().unwrap();
    assert_eq!(published.last_counter, 5);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn reservation_cursor_is_independent_of_private_journal_allocator() {
    let store = Store::default();
    batch(store.clone())
        .write(
            &token(10),
            0,
            &[CausalBatchEdit {
                address: u64::MAX,
                observed: vec![],
                payload: Some(CausalPayload {
                    source: 0,
                    target: 0,
                    name: None,
                }),
            }],
        )
        .unwrap();
    let mut tx = session(store.clone(), 32, 33);
    assert_eq!(tx.create_link(0, 0).unwrap(), 32);
    let rows = tx.rows();
    assert!(tx.ensure_link_created(0).is_err());
    assert_eq!(tx.rows(), rows);
    assert!(tx.ensure_link_created(31).is_err());
    assert!(tx.ensure_link_created(34).is_err());
    assert_eq!(tx.ensure_link_created(33).unwrap(), 33);
    let before = tx.rows();
    assert!(tx.create_link(0, 0).is_err());
    assert_eq!(tx.rows(), before);
    tx.delete_link(u64::MAX).unwrap();
    tx.ensure_link_created(u64::MAX).unwrap();
    tx.set_name(u64::MAX, Some("known identity".into()))
        .unwrap();
    assert_eq!(tx.commit(&token(100)).unwrap().len(), 3);
    let empty = Store::default();
    let mut last = session(empty, u64::MAX, u64::MAX);
    assert_eq!(last.create_link(0, 0).unwrap(), u64::MAX);
    assert!(last.create_link(0, 0).is_err());
    assert_eq!(last.commit(&token(100)).unwrap().len(), 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn cancellation_empty_receipt_and_repeated_cleanup_consume_no_author_counter() {
    let store = seeded();
    let mut tx = session(store.clone(), 32, 33);
    tx.set_name(1, Some("temporary".into())).unwrap();
    tx.set_name(1, Some("old".into())).unwrap();
    let id = tx.create_link(0, 0).unwrap();
    tx.delete_link(id).unwrap();
    let calls = store.0.borrow().calls;
    assert!(tx.commit(&token(100)).unwrap().is_empty());
    assert_eq!(batch(store.clone()).snapshot().unwrap().last_counter, 3);
    assert_eq!(store.0.borrow().calls, calls + 1);
    assert!(tx.commit(&token(100)).unwrap().is_empty());
    assert_eq!(store.0.borrow().calls, calls + 1);
    let mut abandoned = session(store.clone(), 40, 41);
    abandoned.delete_link(1).unwrap();
    abandoned.rollback();
    abandoned.rollback();
    assert!(abandoned.commit(&token(200)).is_err());
    assert_eq!(store.0.borrow().calls, calls + 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn frozen_unknown_requests_retry_exactly_without_rebase_after_following_write() {
    for failure in [1, 2] {
        let store = seeded();
        let mut tx = session(store.clone(), 32, 33);
        tx.set_name(1, Some("new".into())).unwrap();
        store.0.borrow_mut().failure = failure;
        let error = tx.commit(&token(100)).unwrap_err();
        assert!(error.to_string().contains("ndeterminate"));
        let request = tx.prepared().unwrap();
        assert_eq!(request.token, token(100));
        assert_eq!(request.edits.len(), 1);
        assert_eq!(request.edits[0].observed.len(), 1);
        assert!(tx.set_name(1, Some("changed".into())).is_err());
        assert!(tx.commit(&token(200)).is_err());
        if failure == 2 {
            owner(store.clone())
                .write(&token(300), 9, &[], None)
                .unwrap();
        }
        let result = tx.commit(&token(100)).unwrap();
        assert_eq!(result[0].dot.counter, 4);
        let before = stored(&store);
        assert_eq!(tx.commit(&token(100)).unwrap(), result);
        assert_eq!(stored(&store), before);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn losing_deletions_remain_observed_and_constraints_validate_before_publication() {
    let store = seeded();
    let remote = CausalVersion::advance(1, B, 0, &[], None).unwrap();
    owner(store.clone()).receive(&[remote]).unwrap();
    let mut tx = session(store.clone(), 32, 33);
    // Max-dot currently selects Bob's deletion. Explicit recreation observes both siblings.
    assert!(tx.get_link(1).is_none());
    tx.ensure_link_created(1).unwrap();
    tx.set_name(1, Some("recreated".into())).unwrap();
    let request = tx.prepare(&token(100)).unwrap();
    assert_eq!(request.edits[0].observed.len(), 2);
    let written = tx.commit(&token(100)).unwrap();
    assert_eq!(written.len(), 1);
    assert_eq!(
        batch(store.clone())
            .snapshot()
            .unwrap()
            .versions
            .iter()
            .filter(|v| v.address == 1)
            .count(),
        1
    );
    for structure in [
        StateStructure::Flat,
        StateStructure::Shared,
        StateStructure::SortedRuns,
    ] {
        for validation in [ConstraintValidation::Scan, ConstraintValidation::Indexed] {
            let store = Store::default();
            let mut tx = CausalSession::new(
                batch(store.clone()),
                32,
                33,
                CausalConflictPolicy::Reject,
                CausalWorkspaceOptions {
                    state_structure: structure,
                    constraint_validation: validation,
                    constraints: vec![Constraint::UniquePairs],
                    ..Default::default()
                },
            )
            .unwrap();
            tx.create_link(0, 0).unwrap();
            tx.create_link(0, 0).unwrap();
            assert!(tx.commit(&token(100)).is_err());
            assert!(stored(&store).is_empty());
            assert_eq!(store.0.borrow().calls, 0);
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn stale_reservations_and_bounded_final_batches_never_publish_partial_effects() {
    let store = seeded();
    let mut stale = session(store.clone(), 32, 33);
    stale.delete_link(1).unwrap();
    owner(store.clone())
        .write(&token(300), 9, &[], None)
        .unwrap();
    let before = stored(&store);
    let calls = store.0.borrow().calls;
    assert!(stale.commit(&token(100)).is_err());
    assert_eq!(stored(&store), before);
    assert_eq!(store.0.borrow().calls, calls);
    for (first, last) in [(0, 1), (33, 32), (1, 1)] {
        assert!(CausalSession::new(
            batch(store.clone()),
            first,
            last,
            CausalConflictPolicy::Reject,
            Default::default()
        )
        .is_err());
    }
    let store = Store::default();
    let mut full = session(store.clone(), 1, 65);
    for _ in 0..65 {
        full.create_link(0, 0).unwrap();
    }
    assert!(full.commit(&token(100)).is_err());
    assert!(stored(&store).is_empty());
    assert_eq!(store.0.borrow().calls, 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn prepared_requests_and_finished_rows_are_owned_across_repeated_cleanup() {
    let store = seeded();
    let mut tx = session(store.clone(), 32, 33);
    tx.set_name(1, Some("".into())).unwrap();
    let mut request = tx.prepare(&token(100)).unwrap();
    assert_eq!(
        request.edits[0].payload.as_ref().unwrap().name.as_deref(),
        Some("")
    );
    request.edits[0].payload.as_mut().unwrap().name = Some("caller mutation".into());
    request.edits[0].observed.clear();
    let mut rows = tx.rows();
    assert_eq!(rows[0].name.as_deref(), Some(""));
    rows[0].name = None;
    let written = tx.commit(&token(100)).unwrap();
    assert_eq!(
        written[0].payload.as_ref().unwrap().name.as_deref(),
        Some("")
    );
    assert_eq!(tx.rows()[0].name.as_deref(), Some(""));
    let calls = store.0.borrow().calls;
    let retained = tx.rows();
    tx.rollback();
    tx.rollback();
    assert_eq!(retained[0].name.as_deref(), Some(""));
    assert!(tx.rows().is_empty());
    assert!(tx.commit(&token(100)).is_err());
    assert_eq!(store.0.borrow().calls, calls);
    assert_eq!(batch(store).snapshot().unwrap().last_counter, 4);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn token_and_payload_preflight_reject_without_publication() {
    for invalid in [
        "",
        "invalid",
        "00000000000000000000000000000000",
        "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
    ] {
        let store = seeded();
        let before = stored(&store);
        let calls = store.0.borrow().calls;
        let mut tx = session(store.clone(), 32, 33);
        tx.set_name(1, Some("new".into())).unwrap();
        assert!(tx.prepare(invalid).is_err());
        assert!(tx.prepared().is_none());
        assert_eq!(stored(&store), before);
        assert_eq!(store.0.borrow().calls, calls);
        // Invalid identity must not freeze an otherwise valid workspace.
        tx.set_name(1, Some("valid after rejection".into()))
            .unwrap();
        assert_eq!(tx.commit(&token(100)).unwrap().len(), 1);
    }
    let store = seeded();
    let before = stored(&store);
    let calls = store.0.borrow().calls;
    let mut tx = session(store.clone(), 32, 33);
    tx.set_name(1, Some("a".repeat(4097))).unwrap();
    assert!(tx.prepare(&token(100)).is_err());
    assert!(tx.prepared().is_none());
    assert_eq!(stored(&store), before);
    assert_eq!(store.0.borrow().calls, calls);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn standard_observers_and_frozen_reads_keep_owned_values() {
    let store = seeded();
    let mut tx = session(store.clone(), 32, 33);
    let mut observed = Vec::new();
    let before = tx
        .update_link_observed(1, 2, 0, &mut |a, b| observed.push((a, b)))
        .unwrap();
    assert_eq!((before.index, before.source, before.target), (1, 0, 2));
    let deleted = tx
        .delete_link_observed(4, &mut |a, b| observed.push((a, b)))
        .unwrap();
    assert_eq!(deleted.index, 4);
    assert_eq!(observed.len(), 2);
    let mut calls = 0;
    assert!(tx.delete_link_observed(9, &mut |_, _| calls += 1).is_err());
    assert_eq!(calls, 0);
    assert!(tx.flush().is_err());
    tx.set_name(1, Some("final".into())).unwrap();
    tx.prepare(&token(100)).unwrap();
    assert_eq!(tx.get_link(1).unwrap().source, 2);
    assert!(tx.get_link(9).is_none());
    assert_eq!(tx.get_name(1).as_deref(), Some("final"));
    assert!(tx.get_name(9).is_none());
    assert_eq!(tx.get_by_name("final"), Some(1));
    assert!(tx.get_by_name("missing").is_none());
    assert_eq!(tx.all_links().len(), 2);
    assert_eq!(tx.commit(&token(100)).unwrap().len(), 2);
    tx.rollback();
    assert!(tx.get_link(1).is_none());
    assert!(tx.get_name(1).is_none());
    assert!(tx.get_by_name("final").is_none());
    assert!(tx.all_links().is_empty());
    assert!(tx.ensure_link_created(1).is_err());
    let before = stored(&store);
    assert!(CausalSession::new(
        batch(store.clone()),
        40,
        41,
        CausalConflictPolicy::Reject,
        CausalWorkspaceOptions {
            constraints: vec![Constraint::References],
            ..Default::default()
        }
    )
    .is_err());
    assert_eq!(stored(&store), before);
    let empty = Store::default();
    let mut overflow = session(empty.clone(), 1, 2);
    overflow.create_link(0, 0).unwrap();
    overflow.create_link(0, 0).unwrap();
    assert!(overflow.prepare(&token(u128::MAX)).is_err());
    assert!(stored(&empty).is_empty());
    assert_eq!(empty.0.borrow().calls, 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_finite_schedules_match_rows_differences_and_exact_retries() {
    let vectors: Value =
        serde_json::from_str(include_str!("../../docs/protocol/causal-session-v1.json")).unwrap();
    assert_eq!(vectors["cases"].as_array().unwrap().len(), 343);
    for case in vectors["cases"].as_array().unwrap() {
        for failure in [0, 1, 2] {
            let store = seeded();
            let initial = stored(&store);
            let calls = store.0.borrow().calls;
            let mut tx = session(store.clone(), 32, 33);
            for (position, operation) in case["operations"].as_array().unwrap().iter().enumerate() {
                let before = tx.rows();
                let result = match operation.as_str().unwrap() {
                    "create" => tx.create_link(0, 2).map(|_| ()),
                    "rename" => tx.set_name(1, Some("new\u{feff}\0🙂".into())),
                    "update" => tx.update_link(1, 2, 0).map(|_| ()),
                    "delete" => tx.delete_link(1).map(|_| ()),
                    "restore" => tx
                        .ensure_link_created(1)
                        .and_then(|_| tx.update_link(1, 0, 2))
                        .and_then(|_| tx.set_name(1, Some("old".into()))),
                    "cancel-create" => {
                        if tx.get_link(32).is_some() {
                            tx.delete_link(32).map(|_| ())
                        } else {
                            Ok(())
                        }
                    }
                    "noop" => Ok(()),
                    _ => unreachable!(),
                };
                assert_eq!(
                    result.is_err(),
                    case["errors"][position].as_bool().unwrap(),
                    "{case}"
                );
                if result.is_err() {
                    assert_eq!(tx.rows(), before);
                }
                assert_eq!(stored(&store), initial);
                assert_eq!(store.0.borrow().calls, calls);
            }
            let rows = serde_json::to_value(
                tx.rows()
                    .iter()
                    .map(|row| {
                        serde_json::json!([
                            row.link.index.to_string(),
                            row.link.source.to_string(),
                            row.link.target.to_string(),
                            row.name
                        ])
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            assert_eq!(rows, case["rows"], "{case}");
            store.0.borrow_mut().failure = failure;
            let first = tx.commit(&token(100));
            if failure == 0 {
                assert!(first.is_ok());
            } else {
                assert!(first.is_err());
            }
            let written = tx.commit(&token(100)).unwrap();
            assert_eq!(
                serde_json::to_value(
                    written
                        .iter()
                        .map(|v| v.address.to_string())
                        .collect::<Vec<_>>()
                )
                .unwrap(),
                case["changed"]
            );
            assert_eq!(
                batch(store.clone()).snapshot().unwrap().last_counter,
                3 + written.len() as u64
            );
            assert_eq!(tx.commit(&token(100)).unwrap(), written);
            assert_eq!(
                store.0.borrow().calls,
                calls + if failure == 1 { 2 } else { 1 }
            );
        }
    }
}
