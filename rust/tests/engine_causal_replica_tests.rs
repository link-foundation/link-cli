#![cfg(feature = "causal")]
use link_cli::engine::causal_replica::{CausalReplica, CausalReplicaOptions};
use link_cli::engine::{
    causal::CausalPayload,
    cluster::{AtomicJournalStore, JournalVersion},
};
use link_cli::LinkError;
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

const A: &str = "00000000000000000000000000000001";
const B: &str = "00000000000000000000000000000002";
#[derive(Default)]
struct Data {
    version: JournalVersion,
    calls: u64,
    failure: u8,
    weak: bool,
    before_compare: Option<Box<dyn FnOnce()>>,
    corrupt: Option<Vec<JournalVersion>>,
}
#[derive(Clone, Default)]
struct Store(Rc<RefCell<Data>>);
impl AtomicJournalStore for Store {
    fn supports_durable_flush(&self) -> bool {
        !self.0.borrow().weak
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        assert_eq!(keys, ["replica"]);
        let data = self.0.borrow();
        Ok(data
            .corrupt
            .clone()
            .unwrap_or_else(|| vec![data.version.clone()]))
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        assert_eq!(keys, ["replica"]);
        let before = self.0.borrow_mut().before_compare.take();
        if let Some(before) = before {
            before();
        }
        let mut state = self.0.borrow_mut();
        state.calls += 1;
        let failure = std::mem::take(&mut state.failure);
        if failure == 1 {
            return Err(LinkError::StorageError("before publication".into()));
        }
        if state.version.generation != expected[0].generation {
            return Ok(false);
        }
        state.version.generation += 1;
        state.version.entries = updates[0].clone().unwrap();
        if failure == 2 {
            return Err(LinkError::StorageError("lost publication reply".into()));
        }
        Ok(true)
    }
}
fn owner(store: Store, author: &str) -> CausalReplica<Store> {
    CausalReplica::new(store, "replica", author, CausalReplicaOptions::default()).unwrap()
}
fn row(name: &str) -> Option<CausalPayload> {
    Some(CausalPayload {
        source: 2,
        target: 3,
        name: Some(name.into()),
    })
}
fn token(n: u64) -> String {
    format!("{n:032x}")
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_local_journal_vectors() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../docs/protocol/causal-replica-v1.json")).unwrap();
    for fixture in fixtures["cases"].as_array().unwrap() {
        let store = Store::default();
        let entries: Vec<_> = fixture["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect();
        store.0.borrow_mut().version = JournalVersion {
            generation: fixture["generation"].as_str().unwrap().parse().unwrap(),
            entries: entries.clone(),
        };
        let mut options = CausalReplicaOptions::default();
        if let Some(n) = fixture["options"]["maxEntries"].as_u64() {
            options.max_entries = n as usize;
        }
        if let Some(n) = fixture["options"]["maxBytes"].as_u64() {
            options.max_bytes = n as usize;
        }
        let mut replica = CausalReplica::new(store.clone(), "replica", A, options).unwrap();
        let result = replica.snapshot();
        if fixture["valid"] == true {
            let result = result.unwrap_or_else(|e| panic!("{}: {e}", fixture["name"]));
            assert_eq!(
                result.last_counter.to_string(),
                fixture["lastCounter"].as_str().unwrap()
            );
            assert_eq!(
                result
                    .versions
                    .iter()
                    .map(|v| v.encode().unwrap())
                    .collect::<Vec<_>>(),
                fixture["versions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(Value::to_string)
                    .collect::<Vec<_>>()
            );
        } else {
            assert!(result.is_err(), "{}", fixture["name"]);
        }
        assert_eq!(store.0.borrow().calls, 0);
        assert_eq!(store.0.borrow().version.entries, entries);
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn durable_receipts_survive_later_edits_and_before_after_reply_loss() {
    for failure in [1, 2] {
        let store = Store::default();
        store.0.borrow_mut().failure = failure;
        let mut first = owner(store.clone(), A);
        assert!(first
            .write(&token(1), 1, &[], row("uncertain"))
            .unwrap_err()
            .to_string()
            .contains("indeterminate"));
        assert_eq!(store.0.borrow().calls, 1);
        let following = first.write(&token(2), 2, &[], row("following")).unwrap();
        let calls = store.0.borrow().calls;
        let mut reopened = owner(store.clone(), A);
        let retried = reopened.write(&token(1), 1, &[], row("uncertain")).unwrap();
        assert_eq!(
            (following.dot.counter, retried.dot.counter),
            if failure == 1 { (1, 2) } else { (2, 1) }
        );
        assert_eq!(store.0.borrow().calls, calls + u64::from(failure == 1));
        assert_eq!(reopened.snapshot().unwrap().last_counter, 2);
        assert!(reopened
            .write(&token(1), 1, &[], row("changed retry"))
            .is_err());
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn concurrent_deletion_resolution_and_historical_equivocation() {
    let left_store = Store::default();
    let mut left = owner(left_store.clone(), A);
    let mut right = owner(Store::default(), B);
    let initial = left.write(&token(1), 1, &[], row("initial")).unwrap();
    assert!(right.receive(std::slice::from_ref(&initial)).unwrap());
    let update = left
        .write(&token(2), 1, std::slice::from_ref(&initial), row("update"))
        .unwrap();
    let deletion = right
        .write(&token(1), 1, std::slice::from_ref(&initial), None)
        .unwrap();
    left.receive(std::slice::from_ref(&deletion)).unwrap();
    right.receive(std::slice::from_ref(&update)).unwrap();
    assert_eq!(left.read(1).unwrap(), right.read(1).unwrap());
    assert_eq!(left.read(1).unwrap().len(), 2);
    let context = left.read(1).unwrap();
    let resolved = left.write(&token(3), 1, &context, None).unwrap();
    right.receive(std::slice::from_ref(&resolved)).unwrap();
    assert!(!right.receive(std::slice::from_ref(&initial)).unwrap());
    assert_eq!(right.read(1).unwrap(), vec![resolved]);
    let mut forged = deletion;
    forged.payload = row("forged");
    let before = left_store.0.borrow().version.clone();
    assert!(left.receive(&[forged]).is_err());
    assert_eq!(left_store.0.borrow().version, before);
    let recovered = left.into_store();
    assert_eq!(owner(recovered, A).snapshot().unwrap().last_counter, 3);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn original_retry_receipts_and_owned_snapshots_survive_following_edits() {
    let store = Store::default();
    let mut first = owner(store.clone(), A);
    let mut initial = first.write(&token(1), 1, &[], row("initial")).unwrap();
    let edited = first
        .write(&token(2), 1, std::slice::from_ref(&initial), row("edited"))
        .unwrap();
    let mut reopened = owner(store.clone(), A);
    let calls = store.0.borrow().calls;
    assert_eq!(
        initial,
        reopened.write(&token(1), 1, &[], row("initial")).unwrap()
    );
    assert_eq!(calls, store.0.borrow().calls);
    assert!(reopened
        .write(&token(1), 1, &[], row("changed retry"))
        .is_err());
    assert!(reopened
        .write(&token(1), 1, std::slice::from_ref(&initial), row("initial"))
        .is_err());
    initial.clock[0].counter = 99;
    let mut snapshot = reopened.snapshot().unwrap();
    snapshot.versions[0].clock[0].counter = 77;
    assert_eq!(reopened.read(1).unwrap(), vec![edited]);
    assert_eq!(reopened.snapshot().unwrap().last_counter, 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn two_active_handles_fence_global_author_counter_publication() {
    let store = Store::default();
    let mut left = owner(store.clone(), A);
    let mut right = owner(store.clone(), A);
    store.0.borrow_mut().before_compare = Some(Box::new(move || {
        assert_eq!(
            right
                .write(&token(2), 2, &[], row("right"))
                .unwrap()
                .dot
                .counter,
            1
        );
    }));
    assert!(left
        .write(&token(1), 1, &[], row("left"))
        .unwrap_err()
        .to_string()
        .contains("conflict"));
    assert_eq!(store.0.borrow().calls, 2);
    assert_eq!(left.snapshot().unwrap().last_counter, 1);
    assert_eq!(
        left.write(&token(1), 1, &[], row("left"))
            .unwrap()
            .dot
            .counter,
        2
    );
    assert_eq!(left.snapshot().unwrap().versions.len(), 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn journal_admission_and_noncanonical_state_fail_without_publication() {
    for options in [
        CausalReplicaOptions {
            max_entries: 2,
            ..Default::default()
        },
        CausalReplicaOptions {
            max_bytes: 180,
            ..Default::default()
        },
    ] {
        let store = Store::default();
        let mut replica = CausalReplica::new(store.clone(), "replica", A, options).unwrap();
        if options.max_entries == 2 {
            replica.write(&token(1), 1, &[], row("first")).unwrap();
        }
        let before = store.0.borrow().version.clone();
        let calls = store.0.borrow().calls;
        assert!(replica
            .write(&token(2), 2, &[], row(&"x".repeat(300)))
            .is_err());
        assert_eq!(store.0.borrow().version, before);
        assert_eq!(store.0.borrow().calls, calls);
    }
    let store = Store::default();
    let mut replica = owner(store.clone(), A);
    replica.write(&token(1), 1, &[], row("first")).unwrap();
    let original = store.0.borrow().version.entries.clone();
    let calls = store.0.borrow().calls;
    for entries in [
        vec![format!("{} ", original[0]), original[1].clone()],
        vec![original[0].clone(), "{".into()],
        vec![original[0].clone(), format!("{} ", original[1])],
        vec![original[1].clone()],
        vec![
            original[0].clone(),
            original[1].clone(),
            original[1].clone(),
        ],
    ] {
        store.0.borrow_mut().version.entries = entries;
        assert!(replica.snapshot().is_err());
        assert_eq!(store.0.borrow().calls, calls);
    }
    for frame in [
        "[[[[[[]]]]]]".into(),
        format!("\"{}\"", "1".repeat(1048575)),
        format!("[{}]", vec!["null"; 32770].join(",")),
        "{}".into(),
        "}".into(),
        "\"".into(),
        "[".into(),
        "]".into(),
        "\\".into(),
        "\x1f".into(),
        "\x7f".into(),
    ] {
        store.0.borrow_mut().version.entries = vec![original[0].clone(), frame];
        assert!(replica.snapshot().is_err());
        assert_eq!(store.0.borrow().calls, calls);
    }
    store.0.borrow_mut().version.entries = original;
    assert!(owner(store, B).snapshot().is_err());
    use link_cli::engine::causal::{CausalDot, CausalVersion};
    let clock: Vec<_> = (2..66)
        .map(|n| CausalDot {
            author: token(n),
            counter: 1,
        })
        .collect();
    let maximum: Vec<_> = clock
        .iter()
        .enumerate()
        .map(|(index, dot)| CausalVersion {
            address: index as u64 + 1,
            dot: dot.clone(),
            clock: clock.clone(),
            payload: row(&"x".repeat(4096)),
        })
        .collect();
    let large = Store::default();
    assert!(owner(large.clone(), A).receive(&maximum).unwrap());
    assert_eq!(
        owner(large.clone(), A).snapshot().unwrap().versions.len(),
        64
    );
    let state = large.0.borrow();
    assert_eq!(state.version.entries.len(), 2);
    assert!(state.version.entries[1].len() < 1048576);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_arguments_and_atomic_provider_shapes_reject_at_the_boundary() {
    for (key, author, options) in [
        ("", A, CausalReplicaOptions::default()),
        (
            "replica",
            "00000000000000000000000000000000",
            CausalReplicaOptions::default(),
        ),
        (
            "replica",
            A,
            CausalReplicaOptions {
                max_entries: 1,
                ..Default::default()
            },
        ),
        (
            "replica",
            A,
            CausalReplicaOptions {
                max_bytes: 0,
                ..Default::default()
            },
        ),
    ] {
        assert!(CausalReplica::new(Store::default(), key, author, options).is_err());
    }
    let store = Store::default();
    store.0.borrow_mut().weak = true;
    assert!(CausalReplica::new(
        store.clone(),
        "replica",
        A,
        CausalReplicaOptions {
            require_durable_flush: true,
            ..Default::default()
        }
    )
    .is_err());
    let mut replica = owner(store.clone(), A);
    assert!(replica.snapshot().unwrap().versions.is_empty());
    assert!(replica.read(0).is_err());
    assert!(replica.write("invalid", 1, &[], None).is_err());
    assert!(replica.write(&token(1), 0, &[], None).is_err());
    for corrupt in [
        vec![],
        vec![JournalVersion {
            generation: 0,
            entries: vec!["unexpected".into()],
        }],
    ] {
        store.0.borrow_mut().corrupt = Some(corrupt);
        assert!(replica.snapshot().is_err());
    }
    assert_eq!(store.0.borrow().calls, 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn mixed_receive_batches_validate_all_history_and_sort_canonical_entries() {
    use link_cli::engine::causal::{CausalDot, CausalVersion};
    let third = "00000000000000000000000000000003";
    let remote = |counter: u64| CausalVersion::advance(1, B, counter - 1, &[], None).unwrap();
    let other = CausalVersion::advance(1, third, 0, &[], None).unwrap();
    for batch in [
        vec![remote(1), remote(2), other.clone()],
        vec![other.clone(), remote(2), remote(1)],
    ] {
        let store = Store::default();
        let mut replica = owner(store.clone(), A);
        assert!(replica.receive(&batch).unwrap());
        assert_eq!(replica.read(1).unwrap(), vec![remote(2), other.clone()]);
        assert_eq!(
            owner(store, A).snapshot().unwrap().versions,
            vec![remote(2), other.clone()]
        );
    }
    let store = Store::default();
    let mut replica = owner(store.clone(), A);
    let clock = vec![
        CausalDot {
            author: B.into(),
            counter: 2,
        },
        CausalDot {
            author: third.into(),
            counter: 1,
        },
    ];
    let one = CausalVersion {
        address: 2,
        dot: clock[0].clone(),
        clock: clock.clone(),
        payload: None,
    };
    replica.receive(std::slice::from_ref(&one)).unwrap();
    let before = store.0.borrow().version.clone();
    let two = CausalVersion {
        dot: clock[1].clone(),
        ..one
    };
    assert!(replica.receive(&[two]).is_err());
    assert_eq!(store.0.borrow().version, before);
    assert!(replica.receive(&vec![remote(1); 65]).is_err());
    let mut malformed = remote(1);
    malformed.address = 0;
    assert!(replica.receive(&[remote(1), malformed]).is_err());
    assert!(replica
        .receive(&[CausalVersion::advance(2, A, 8, &[], None).unwrap()])
        .is_err());
    assert_eq!(store.0.borrow().version, before);
    let remote = CausalVersion::advance(1, B, 0, &[], row("remote")).unwrap();
    let later =
        CausalVersion::advance(1, B, 1, std::slice::from_ref(&remote), row("later")).unwrap();
    let mut forged = remote.clone();
    forged.payload = row("forged");
    let history = Store::default();
    let mut observing = owner(history.clone(), A);
    observing.receive(std::slice::from_ref(&remote)).unwrap();
    let resolved = observing
        .write(&token(1), 1, std::slice::from_ref(&remote), None)
        .unwrap();
    let before = history.0.borrow().version.clone();
    assert!(observing
        .write(&token(2), 1, &[forged.clone(), resolved], None)
        .is_err());
    assert_eq!(history.0.borrow().version, before);
    let unreceived = Store::default();
    let mut observing = owner(unreceived.clone(), A);
    let mutation = observing
        .write(&token(1), 1, &[later.clone(), remote.clone(), remote], None)
        .unwrap();
    let before = unreceived.0.borrow().version.clone();
    let publications = unreceived.0.borrow().calls;
    assert!(owner(unreceived.clone(), A)
        .receive(std::slice::from_ref(&forged))
        .is_err());
    assert!(owner(unreceived.clone(), A)
        .write(&token(1), 1, &[forged, later.clone()], None)
        .is_err());
    assert_eq!(
        observing.write(&token(1), 1, &[later], None).unwrap(),
        mutation
    );
    assert_eq!(unreceived.0.borrow().version, before);
    assert_eq!(unreceived.0.borrow().calls, publications);
}
