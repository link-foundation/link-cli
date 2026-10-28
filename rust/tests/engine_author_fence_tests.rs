#![cfg(feature = "author-fencing")]
use link_cli::engine::{
    author_fence::AuthorFence,
    cluster::{AtomicJournalStore, JournalVersion},
};
use link_cli::LinkError;
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
const A: &str = "00000000000000000000000000000001";
#[derive(Default)]
struct Data {
    values: BTreeMap<String, JournalVersion>,
    calls: usize,
    failure: u8,
    before: Option<Box<dyn FnOnce()>>,
    corrupt: Option<Vec<JournalVersion>>,
    volatile: bool,
    read_error: bool,
}
#[derive(Clone, Default)]
struct Store(Rc<RefCell<Data>>);
impl AtomicJournalStore for Store {
    fn supports_durable_flush(&self) -> bool {
        !self.0.borrow().volatile
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        let data = self.0.borrow();
        if data.read_error {
            return Err(LinkError::StorageError("read failure".into()));
        }
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
            if let Some(update) = update {
                data.values.insert(
                    key.clone(),
                    JournalVersion {
                        generation: value.generation + 1,
                        entries: update.clone(),
                    },
                );
            }
        }
        if failure == 2 {
            return Err(LinkError::StorageError("after".into()));
        }
        Ok(true)
    }
}

fn fence(store: Store) -> AuthorFence<Store> {
    AuthorFence::new(store, "fence", A, Default::default()).unwrap()
}
fn token(value: u128) -> String {
    format!("{value:032x}")
}
fn stored(store: &Store) -> BTreeMap<String, JournalVersion> {
    store.0.borrow().values.clone()
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn canonical_claim_journal_fixtures_are_read_only() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../docs/protocol/author-fence-v1.json")).unwrap();
    for v in fixtures["cases"].as_array().unwrap() {
        let s = Store::default();
        s.0.borrow_mut().values.insert(
            "fence".into(),
            JournalVersion {
                generation: v["journal"]["generation"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap(),
                entries: v["journal"]["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap().into())
                    .collect(),
            },
        );
        let before = stored(&s);
        let result = fence(s.clone()).snapshot();
        if v["valid"] == true {
            let result = result.unwrap();
            assert_eq!(result.epoch.to_string(), v["epoch"].as_str().unwrap());
            assert_eq!(result.claims.len(), v["claims"].as_array().unwrap().len());
        } else {
            assert!(result.is_err(), "{}", v["name"]);
        }
        assert_eq!(stored(&s), before);
        assert_eq!(s.0.borrow().calls, 0);
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn explicit_takeover_fences_old_reads_writes_and_receipts() {
    let s = Store::default();
    let mut f = fence(s.clone());
    let first = f.claim(0, &token(10)).unwrap();
    let mut old = fence(s.clone()).bind(first.clone()).unwrap();
    let keys = vec!["data".into()];
    let expected = old.read(&keys).unwrap();
    assert!(old
        .compare_exchange(&keys, &expected, &[Some(vec!["one".into()])])
        .unwrap());
    let second = f.claim(1, &token(20)).unwrap();
    let mut current = fence(s.clone()).bind(second).unwrap();
    let before = stored(&s);
    let calls = s.0.borrow().calls;
    assert!(old.read(&keys).is_err());
    assert!(old
        .compare_exchange(&keys, &expected, &[Some(vec!["stale".into()])])
        .is_err());
    assert!(f.claim(0, &token(10)).is_err());
    assert_eq!(stored(&s), before);
    assert_eq!(s.0.borrow().calls, calls);
    assert_eq!(current.read(&keys).unwrap()[0].entries, vec!["one"]);
    assert_eq!(current.into_store().0.borrow().calls, calls);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn takeover_inside_guarded_data_cas_is_atomic() {
    let s = Store::default();
    let first = fence(s.clone()).claim(0, &token(10)).unwrap();
    let mut old = fence(s.clone()).bind(first).unwrap();
    let keys = vec!["a".into(), "b".into()];
    let expected = old.read(&keys).unwrap();
    let nested = s.clone();
    s.0.borrow_mut().before = Some(Box::new(move || {
        fence(nested).claim(1, &token(20)).unwrap();
    }));
    assert!(!old
        .compare_exchange(
            &keys,
            &expected,
            &[Some(vec!["a".into()]), Some(vec!["b".into()])]
        )
        .unwrap());
    assert!(!s.0.borrow().values.contains_key("a"));
    assert!(!s.0.borrow().values.contains_key("b"));
    assert!(old.read(&keys).is_err());
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn uncertain_claims_and_competing_predecessors_preserve_original_identity() {
    for failure in [1, 2] {
        let s = Store::default();
        s.0.borrow_mut().failure = failure;
        let mut f = fence(s.clone());
        assert!(f
            .claim(0, &token(10))
            .unwrap_err()
            .to_string()
            .contains("indeterminate"));
        assert_eq!(s.0.borrow().calls, 1);
        assert_eq!(f.claim(0, &token(10)).unwrap().epoch, 1);
        f.claim(1, &token(20)).unwrap();
        let before = stored(&s);
        assert!(f.claim(0, &token(10)).is_err());
        assert_eq!(stored(&s), before);
    }
    let s = Store::default();
    let nested = s.clone();
    s.0.borrow_mut().before = Some(Box::new(move || {
        fence(nested).claim(0, &token(20)).unwrap();
    }));
    assert!(fence(s.clone()).claim(0, &token(10)).is_err());
    assert_eq!(fence(s).snapshot().unwrap().epoch, 1);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_budgets_and_read_only_guards_preserve_active_retry() {
    let s = Store::default();
    let mut f = AuthorFence::new(
        s.clone(),
        "fence",
        A,
        link_cli::engine::author_fence::AuthorFenceOptions {
            max_entries: 2,
            ..Default::default()
        },
    )
    .unwrap();
    let first = f.claim(0, &token(10)).unwrap();
    let mut store = fence(s.clone()).bind(first.clone()).unwrap();
    let keys = vec!["data".into()];
    let before = stored(&s);
    let expected = store.read(&keys).unwrap();
    assert!(store.compare_exchange(&keys, &expected, &[None]).unwrap());
    assert_eq!(stored(&s), before);
    assert!(f.claim(1, &token(20)).is_err());
    assert_eq!(f.claim(0, &token(10)).unwrap(), first);
    assert_eq!(stored(&s), before);
    s.0.borrow_mut().values.get_mut("fence").unwrap().generation = u64::MAX;
    assert!(f.claim(1, &token(20)).is_err());
    assert_eq!(f.claim(0, &token(10)).unwrap(), first);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn malformed_admission_and_provider_snapshots_cannot_publish() {
    use link_cli::engine::author_fence::{AuthorClaim, AuthorFenceOptions};
    let s = Store::default();
    assert!(AuthorFence::new(s.clone(), "bad/key", A, Default::default()).is_err());
    assert!(AuthorFence::new(s.clone(), "fence", "bad", Default::default()).is_err());
    for options in [
        AuthorFenceOptions {
            max_entries: 1,
            ..Default::default()
        },
        AuthorFenceOptions {
            max_bytes: 0,
            ..Default::default()
        },
    ] {
        assert!(AuthorFence::new(s.clone(), "fence", A, options).is_err());
    }
    s.0.borrow_mut().volatile = true;
    assert!(AuthorFence::new(
        s.clone(),
        "fence",
        A,
        AuthorFenceOptions {
            require_durable_flush: true,
            ..Default::default()
        }
    )
    .is_err());
    let mut f = fence(s.clone());
    assert!(f.claim(0, "bad").is_err());
    assert!(f.claim(1, &token(10)).is_err());
    assert!(fence(s.clone())
        .bind(AuthorClaim {
            author: A.into(),
            token: token(10),
            epoch: 0
        })
        .is_err());
    s.0.borrow_mut().corrupt = Some(vec![]);
    assert!(f.snapshot().is_err());
    s.0.borrow_mut().corrupt = None;
    s.0.borrow_mut().read_error = true;
    assert!(f.snapshot().is_err());
    s.0.borrow_mut().read_error = false;
    let claim = f.claim(0, &token(10)).unwrap();
    let mut guarded = fence(s.clone()).bind(claim).unwrap();
    assert!(!guarded.supports_durable_flush());
    s.0.borrow_mut().volatile = false;
    assert!(guarded.supports_durable_flush());
    let before = stored(&s);
    let calls = s.0.borrow().calls;
    for keys in [
        vec![],
        vec!["fence".into()],
        vec!["x".into(), "x".into()],
        (0..32).map(|i| format!("x{i}")).collect(),
    ] {
        assert!(guarded.read(&keys).is_err());
    }
    assert!(guarded
        .compare_exchange(&["data".into()], &[], &[])
        .is_err());
    assert_eq!(stored(&s), before);
    assert_eq!(s.0.borrow().calls, calls);
    assert_eq!(fence(s.clone()).into_store().0.borrow().calls, calls);
}
#[cfg(all(feature = "causal-batches", feature = "address-reservations"))]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn takeover_resolves_decided_batch_and_burns_old_reservation_suffix() {
    use link_cli::engine::{
        address_allocator::AddressAllocator,
        causal::CausalPayload,
        causal_batch::{CausalBatch, CausalBatchEdit},
    };
    let s = Store::default();
    let mut f = fence(s.clone());
    let first = f.claim(0, &token(10)).unwrap();
    let mut allocator = AddressAllocator::new(
        fence(s.clone()).bind(first.clone()).unwrap(),
        "addresses",
        1,
        100,
        Default::default(),
    )
    .unwrap();
    assert_eq!(allocator.reserve(A, &token(110), 4).unwrap().last, 4);
    let mut old = CausalBatch::new(
        fence(s.clone()).bind(first).unwrap(),
        "owner",
        "receipts",
        A,
        Default::default(),
    )
    .unwrap();
    let edits = vec![CausalBatchEdit {
        address: 1,
        observed: vec![],
        payload: Some(CausalPayload {
            source: 0,
            target: 0,
            name: None,
        }),
    }];
    s.0.borrow_mut().failure = 2;
    assert!(old.write(&token(300), 0, &edits).is_err());
    let next = f.claim(1, &token(20)).unwrap();
    let before = stored(&s);
    assert!(old.write(&token(300), 0, &edits).is_err());
    assert_eq!(stored(&s), before);
    let mut current = CausalBatch::new(
        fence(s.clone()).bind(next.clone()).unwrap(),
        "owner",
        "receipts",
        A,
        Default::default(),
    )
    .unwrap();
    assert_eq!(
        current.write(&token(300), 0, &edits).unwrap()[0]
            .dot
            .counter,
        1
    );
    let mut allocator = AddressAllocator::new(
        fence(s.clone()).bind(next).unwrap(),
        "addresses",
        1,
        100,
        Default::default(),
    )
    .unwrap();
    let grant = allocator.reserve(A, &token(210), 2).unwrap();
    assert_eq!(grant.first, 5);
    let base = current.snapshot().unwrap().generation;
    current
        .write(
            &token(400),
            base,
            &[CausalBatchEdit {
                address: grant.first,
                observed: vec![],
                payload: edits[0].payload.clone(),
            }],
        )
        .unwrap();
    assert_eq!(current.snapshot().unwrap().last_counter, 2);
    assert_eq!(stored(&s)["fence"], before["fence"]);
}
