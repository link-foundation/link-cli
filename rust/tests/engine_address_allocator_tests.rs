#![cfg(feature = "address-reservations")]
use link_cli::engine::{
    address_allocator::{AddressAllocator, AddressAllocatorOptions},
    cluster::{AtomicJournalStore, JournalVersion},
};
use link_cli::LinkError;
use serde_json::{json, Value};
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

fn allocator(store: Store) -> AddressAllocator<Store> {
    AddressAllocator::new(store, "addresses", 1, 20, Default::default()).unwrap()
}
fn token(value: u128) -> String {
    format!("{value:032x}")
}
fn stored(store: &Store) -> BTreeMap<String, JournalVersion> {
    store.0.borrow().values.clone()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_canonical_journal_and_admission_vectors_preserve_bytes() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../docs/protocol/address-reservations-v1.json"
    ))
    .unwrap();
    for fixture in fixtures["cases"].as_array().unwrap() {
        let store = Store::default();
        store.0.borrow_mut().values.insert(
            "addresses".into(),
            JournalVersion {
                generation: fixture["journal"]["generation"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap(),
                entries: fixture["journal"]["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().into())
                    .collect(),
            },
        );
        let options = AddressAllocatorOptions {
            max_entries: fixture["options"]["maxEntries"].as_u64().unwrap_or(4096) as usize,
            max_bytes: fixture["options"]["maxBytes"].as_u64().unwrap_or(1_048_576) as usize,
            ..Default::default()
        };
        let before = stored(&store);
        let result = AddressAllocator::new(
            store.clone(),
            "addresses",
            fixture["first"].as_str().unwrap().parse().unwrap(),
            fixture["last"].as_str().unwrap().parse().unwrap(),
            options,
        )
        .unwrap()
        .snapshot();
        if fixture["valid"].as_bool().unwrap() {
            let snapshot = result.unwrap_or_else(|error| panic!("{}: {error}", fixture["name"]));
            assert_eq!(
                snapshot.generation.to_string(),
                fixture["journal"]["generation"]
            );
            assert_eq!(json!(snapshot.next.map(|v| v.to_string())), fixture["next"]);
            let reservations: Vec<_> = snapshot.reservations.iter().map(|v| json!({"author":v.author,"token":v.token,"count":v.count.to_string(),"first":v.first.to_string(),"last":v.last.to_string()})).collect();
            assert_eq!(
                json!(reservations),
                fixture["reservations"],
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
fn exact_retry_survives_uncertain_replies_and_following_grants() {
    for failure in [0, 1, 2] {
        let store = Store::default();
        store.0.borrow_mut().failure = failure;
        let result = allocator(store.clone()).reserve(A, &token(10), 3);
        assert_eq!(result.is_err(), failure != 0);
        if failure != 0 {
            assert!(result.unwrap_err().to_string().contains("indeterminate"));
        }
        assert_eq!(store.0.borrow().calls, 1);
        if failure == 1 {
            assert!(stored(&store).is_empty());
        }
        let mut first = allocator(store.clone()).reserve(A, &token(10), 3).unwrap();
        assert_eq!((first.first, first.last), (1, 3));
        assert_eq!(
            allocator(store.clone())
                .reserve(B, &token(20), 2)
                .unwrap()
                .first,
            4
        );
        let before = stored(&store);
        let calls = store.0.borrow().calls;
        assert_eq!(
            allocator(store.clone()).reserve(A, &token(10), 3).unwrap(),
            first
        );
        assert!(allocator(store.clone()).reserve(B, &token(10), 3).is_err());
        assert!(allocator(store.clone()).reserve(A, &token(10), 2).is_err());
        first.first = 19;
        assert_eq!(first.first, 19);
        let mut snapshot = allocator(store.clone()).snapshot().unwrap();
        snapshot.reservations[0].last = 19;
        assert_eq!(
            allocator(store.clone()).snapshot().unwrap().reservations[0].last,
            3
        );
        assert_eq!(stored(&store), before);
        assert_eq!(store.0.borrow().calls, calls);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn same_base_race_conflicts_then_explicit_retry_gets_disjoint_range() {
    let store = Store::default();
    let other = store.clone();
    store.0.borrow_mut().before = Some(Box::new(move || {
        allocator(other).reserve(B, B, 2).unwrap();
    }));
    assert!(allocator(store.clone())
        .reserve(A, A, 2)
        .unwrap_err()
        .to_string()
        .contains("conflict"));
    assert_eq!(allocator(store.clone()).reserve(A, A, 2).unwrap().first, 3);
    assert_eq!(allocator(store.clone()).reserve(B, B, 2).unwrap().first, 1);
    assert_eq!(
        allocator(store)
            .snapshot()
            .unwrap()
            .reservations
            .iter()
            .map(|v| (v.first, v.last))
            .collect::<Vec<_>>(),
        [(1, 2), (3, 4)]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn full_unsigned_width_exhaustion_does_not_cycle_or_recycle() {
    let store = Store::default();
    let mut owner = AddressAllocator::new(
        store.clone(),
        "addresses",
        u64::MAX - 1,
        u64::MAX,
        Default::default(),
    )
    .unwrap();
    assert_eq!(owner.reserve(A, A, 1).unwrap().first, u64::MAX - 1);
    assert_eq!(owner.reserve(B, B, 1).unwrap().last, u64::MAX);
    let before = stored(&store);
    let calls = store.0.borrow().calls;
    assert_eq!(owner.snapshot().unwrap().next, None);
    assert!(owner.reserve(A, &token(3), 1).is_err());
    assert_eq!(owner.reserve(B, B, 1).unwrap().last, u64::MAX);
    assert_eq!(stored(&store), before);
    assert_eq!(store.0.borrow().calls, calls);
    let mut whole = AddressAllocator::new(
        Store::default(),
        "addresses",
        1,
        u64::MAX,
        Default::default(),
    )
    .unwrap();
    assert_eq!(whole.reserve(A, A, u64::MAX).unwrap().last, u64::MAX);
    assert_eq!(whole.snapshot().unwrap().next, None);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_admission_allows_old_retries_without_publishing() {
    for options in [
        AddressAllocatorOptions {
            max_entries: 2,
            ..Default::default()
        },
        AddressAllocatorOptions {
            max_bytes: 170,
            ..Default::default()
        },
    ] {
        let store = Store::default();
        let mut owner = AddressAllocator::new(store.clone(), "addresses", 1, 20, options).unwrap();
        owner.reserve(A, A, 1).unwrap();
        let before = stored(&store);
        let calls = store.0.borrow().calls;
        assert!(owner.reserve(B, B, 1).is_err());
        assert_eq!(owner.reserve(A, A, 1).unwrap().first, 1);
        assert_eq!(stored(&store), before);
        assert_eq!(store.0.borrow().calls, calls);
    }
    let tiny = Store::default();
    assert!(AddressAllocator::new(
        tiny.clone(),
        "addresses",
        1,
        20,
        AddressAllocatorOptions {
            max_bytes: 1,
            ..Default::default()
        }
    )
    .unwrap()
    .reserve(A, A, 1)
    .is_err());
    assert_eq!(tiny.0.borrow().calls, 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bad_configuration_requests_and_provider_frames_cannot_publish() {
    let store = Store::default();
    for (first, last) in [(0, 1), (2, 1)] {
        assert!(
            AddressAllocator::new(store.clone(), "addresses", first, last, Default::default())
                .is_err()
        );
    }
    for key in ["".into(), "bad/key".into(), "x".repeat(129)] {
        assert!(AddressAllocator::new(store.clone(), &key, 1, 20, Default::default()).is_err());
    }
    for options in [
        AddressAllocatorOptions {
            max_entries: 1,
            ..Default::default()
        },
        AddressAllocatorOptions {
            max_entries: 4097,
            ..Default::default()
        },
        AddressAllocatorOptions {
            max_bytes: 0,
            ..Default::default()
        },
        AddressAllocatorOptions {
            max_bytes: 1_048_577,
            ..Default::default()
        },
    ] {
        assert!(AddressAllocator::new(store.clone(), "addresses", 1, 20, options).is_err());
    }
    let durable = AddressAllocatorOptions {
        require_durable_flush: true,
        ..Default::default()
    };
    AddressAllocator::new(store.clone(), ".valid_-09Z", 1, 20, durable.clone())
        .unwrap()
        .into_store();
    store.0.borrow_mut().volatile = true;
    assert!(AddressAllocator::new(store.clone(), "addresses", 1, 20, durable).is_err());
    let mut owner = allocator(store.clone());
    for (author, token, count) in [
        ("", A, 1),
        ("00000000000000000000000000000000", A, 1),
        ("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF", A, 1),
        (A, "bad", 1),
        (A, A, 0),
    ] {
        assert!(owner.reserve(author, token, count).is_err());
    }
    for result in [vec![], vec![JournalVersion::default(); 2]] {
        store.0.borrow_mut().corrupt = Some(result);
        assert!(owner.snapshot().is_err());
    }
    store.0.borrow_mut().corrupt = None;
    store.0.borrow_mut().read_error = true;
    assert!(owner.reserve(A, A, 1).is_err());
    store.0.borrow_mut().read_error = false;
    store.0.borrow_mut().values.insert(
        "addresses".into(),
        JournalVersion {
            generation: u64::MAX,
            entries: vec!["[\"address-reservations1\",\"1\",\"20\"]".into()],
        },
    );
    assert!(owner.reserve(A, A, 1).is_err());
    assert_eq!(store.0.borrow().calls, 0);
}
