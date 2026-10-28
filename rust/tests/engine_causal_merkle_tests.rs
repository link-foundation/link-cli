#![cfg(feature = "causal-merkle")]
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_merkle::{
        pull_causal, CausalMerkleLeaf, CausalMerkleNode, CausalMerkleOptions, CausalMerkleSnapshot,
        CausalMerkleSource, CausalPullOptions,
    },
    causal_replica::{CausalReplica, CausalReplicaOptions},
    cluster::{AtomicJournalStore, JournalVersion},
};
use link_cli::LinkError;
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

fn fixtures() -> Value {
    serde_json::from_str(include_str!("../../docs/protocol/causal-merkle-v1.json")).unwrap()
}
fn values(items: &Value) -> Vec<CausalVersion> {
    items
        .as_array()
        .unwrap()
        .iter()
        .map(|value| CausalVersion::decode(&value.to_string()).unwrap())
        .collect()
}
fn proof(items: &Value) -> Vec<Vec<String>> {
    items
        .as_array()
        .unwrap()
        .iter()
        .map(|level| {
            level
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().into())
                .collect()
        })
        .collect()
}
fn wires(values: &[CausalVersion]) -> Vec<String> {
    let mut result: Vec<_> = values.iter().map(|v| v.encode().unwrap()).collect();
    result.sort();
    result
}
fn snapshot(values: &[CausalVersion]) -> CausalMerkleSnapshot {
    CausalMerkleSnapshot::new(values, CausalMerkleOptions::default()).unwrap()
}
fn token(n: u64) -> String {
    format!("{n:032x}")
}
#[derive(Default)]
struct Data {
    version: JournalVersion,
    calls: usize,
    fault: u8,
}
#[derive(Clone, Default)]
struct Store(Rc<RefCell<Data>>);
impl AtomicJournalStore for Store {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn read(&mut self, _: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        Ok(vec![self.0.borrow().version.clone()])
    }
    fn compare_exchange(
        &mut self,
        _: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        let mut data = self.0.borrow_mut();
        data.calls += 1;
        let fault = std::mem::take(&mut data.fault);
        if fault == 1 {
            return Err(LinkError::StorageError("before".into()));
        }
        if data.version.generation != expected[0].generation {
            return Ok(false);
        }
        data.version.generation += 1;
        data.version.entries = updates[0].clone().unwrap();
        if fault == 2 {
            return Err(LinkError::StorageError("lost reply".into()));
        }
        Ok(true)
    }
}
fn owner(store: Store) -> CausalReplica<Store> {
    CausalReplica::new(
        store,
        "replica",
        "0000000000000000000000000000000e",
        CausalReplicaOptions::default(),
    )
    .unwrap()
}
type NodeCallback = Box<dyn FnMut(&str) -> Result<CausalMerkleNode, LinkError>>;
type LeafCallback = Box<dyn FnMut(u64) -> Result<CausalMerkleLeaf, LinkError>>;
struct Remote {
    snapshot: Rc<CausalMerkleSnapshot>,
    on_node: Option<NodeCallback>,
    on_leaf: Option<LeafCallback>,
}
impl Remote {
    fn new(snapshot: Rc<CausalMerkleSnapshot>) -> Self {
        Self {
            snapshot,
            on_node: None,
            on_leaf: None,
        }
    }
}
impl CausalMerkleSource for Remote {
    fn root(&self) -> &str {
        self.snapshot.root()
    }
    fn node(&mut self, path: &str) -> Result<CausalMerkleNode, LinkError> {
        if let Some(callback) = self.on_node.as_mut() {
            callback(path)
        } else {
            self.snapshot.node(path)
        }
    }
    fn leaf(&mut self, key: u64) -> Result<CausalMerkleLeaf, LinkError> {
        if let Some(callback) = self.on_leaf.as_mut() {
            callback(key)
        } else {
            self.snapshot.leaf(key)
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_canonical_roots_and_proofs() {
    let cases = fixtures();
    let snapshots: Vec<_> = cases["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let state = snapshot(&values(&case["versions"]));
            assert_eq!(state.root(), case["root"].as_str().unwrap());
            assert_eq!(
                CausalMerkleSnapshot::verify_node(state.root(), &state.node("").unwrap())
                    .unwrap()
                    .len(),
                16
            );
            state
        })
        .collect();
    for case in cases["leaves"].as_array().unwrap() {
        let expected = CausalMerkleLeaf {
            root: case["root"].as_str().unwrap().into(),
            address: case["address"].as_str().unwrap().parse().unwrap(),
            versions: values(&case["versions"]),
            proof: proof(&case["proof"]),
        };
        assert_eq!(
            snapshots[case["snapshot"].as_u64().unwrap() as usize]
                .leaf(expected.address)
                .unwrap(),
            expected
        );
        assert_eq!(
            CausalMerkleSnapshot::verify_leaf(&expected.root, &expected).unwrap(),
            expected.versions
        );
        assert_eq!(
            CausalMerkleSnapshot::encode_leaf(&expected).unwrap(),
            case["wire"].as_str().unwrap()
        );
        assert_eq!(
            CausalMerkleSnapshot::decode_leaf(&expected.root, case["wire"].as_str().unwrap())
                .unwrap(),
            expected
        );
    }
    for case in cases["nodes"].as_array().unwrap() {
        let expected = CausalMerkleNode {
            root: case["root"].as_str().unwrap().into(),
            prefix: case["prefix"].as_str().unwrap().into(),
            children: case["children"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().into())
                .collect(),
            proof: proof(&case["proof"]),
        };
        assert_eq!(
            snapshots[case["snapshot"].as_u64().unwrap() as usize]
                .node(&expected.prefix)
                .unwrap(),
            expected
        );
        assert_eq!(
            CausalMerkleSnapshot::verify_node(&expected.root, &expected).unwrap(),
            expected.children
        );
        assert_eq!(
            CausalMerkleSnapshot::encode_node(&expected).unwrap(),
            case["wire"].as_str().unwrap()
        );
        assert_eq!(
            CausalMerkleSnapshot::decode_node(&expected.root, case["wire"].as_str().unwrap())
                .unwrap(),
            expected
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn snapshot_and_messages_own_nested_state() {
    let cases = fixtures();
    let mut input = values(&cases["snapshots"][3]["versions"]);
    let state = snapshot(&input);
    input[0].clock[0].counter = 99;
    input[0].payload = None;
    assert_eq!(
        state.root(),
        cases["snapshots"][3]["root"].as_str().unwrap()
    );
    let mut leaf = state.leaf(1).unwrap();
    leaf.versions[0].clock[0].counter = 99;
    leaf.proof[0][0] = "0".repeat(64);
    let mut node = state.node("").unwrap();
    node.children[0] = "0".repeat(64);
    assert_eq!(
        CausalMerkleSnapshot::verify_leaf(state.root(), &state.leaf(1).unwrap()).unwrap(),
        values(&cases["snapshots"][3]["versions"])
    );
    CausalMerkleSnapshot::verify_node(state.root(), &state.node("").unwrap()).unwrap();
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn malformed_mixed_and_changed_proofs_reject() {
    let state = snapshot(&values(&fixtures()["snapshots"][3]["versions"]));
    let leaf = state.leaf(1).unwrap();
    for change in 0..11 {
        let mut bad = leaf.clone();
        match change {
            0 => bad.root = "0".repeat(64),
            1 => bad.address = 0,
            2 => bad.address = 2,
            3 => bad.versions.reverse(),
            4 => bad.versions.push(bad.versions[0].clone()),
            5 => {
                bad.versions[0].payload = Some(CausalPayload {
                    source: 1,
                    target: 1,
                    name: Some("forged".into()),
                })
            }
            6 => {
                bad.proof.pop();
            }
            7 => {
                bad.proof[0].pop();
            }
            8 => bad.proof[0][0] = "0".repeat(64),
            9 => bad.proof[0][0] = "A".repeat(64),
            _ => bad.proof[0][0] = "bad".into(),
        }
        assert!(CausalMerkleSnapshot::verify_leaf(state.root(), &bad).is_err());
    }
    let node = state.node("000000000000000").unwrap();
    for change in 0..8 {
        let mut bad = node.clone();
        match change {
            0 => bad.root = "0".repeat(64),
            1 => bad.prefix = "F".into(),
            2 => bad.prefix = "0".repeat(16),
            3 => bad.prefix = format!("1{}", &bad.prefix[1..]),
            4 => {
                bad.children.pop();
            }
            5 => bad.children[0] = "0".repeat(64),
            6 => bad.children[0] = "bad".into(),
            _ => {
                bad.proof.pop();
            }
        }
        assert!(CausalMerkleSnapshot::verify_node(state.root(), &bad).is_err());
    }
    assert!(CausalMerkleSnapshot::verify_leaf("bad", &leaf).is_err());
    assert!(CausalMerkleSnapshot::verify_node("bad", &node).is_err());
    for wire in [
        "".into(),
        "null".into(),
        "[]".into(),
        "{}".into(),
        "[\"other\"]".into(),
        "[".into(),
        "]".into(),
        "\"".into(),
        "[\"\\\\\"]".into(),
        "[\"雪\"]".into(),
        "[\"\n\"]".into(),
        "[".repeat(6) + &"]".repeat(6),
        String::from("[") + &",".repeat(32769) + "]",
        "x".repeat(1_081_345),
    ] {
        assert!(CausalMerkleSnapshot::decode_leaf(state.root(), &wire).is_err());
        assert!(CausalMerkleSnapshot::decode_node(state.root(), &wire).is_err());
    }
    let cases = fixtures();
    for case in cases["leaves"].as_array().unwrap() {
        let wire = case["wire"].as_str().unwrap();
        let expected_root = case["root"].as_str().unwrap();
        for changed in [
            format!("{wire} "),
            wire.replace("causal-leaf1", "causal-leaf2"),
            wire.replace("causal-leaf1", "causal-\\u006ceaf1"),
        ] {
            assert!(CausalMerkleSnapshot::decode_leaf(expected_root, &changed).is_err());
        }
        let original: Value = serde_json::from_str(wire).unwrap();
        for (index, changed) in [
            (1, Value::Null),
            (2, serde_json::json!(1)),
            (2, serde_json::json!("")),
            (2, serde_json::json!("100000000000000000000")),
            (2, serde_json::json!("01")),
            (2, serde_json::json!("0")),
            (2, serde_json::json!("18446744073709551616")),
            (3, Value::Null),
            (
                3,
                serde_json::json!(vec![cases["snapshots"][1]["versions"][0].clone(); 65]),
            ),
            (4, Value::Null),
        ] {
            let mut bad = original.clone();
            bad[index] = changed;
            assert!(
                CausalMerkleSnapshot::decode_leaf(expected_root, &bad.to_string()).is_err(),
                "invalid leaf field {index}: {}",
                bad[index]
            );
        }
    }
    let wire = cases["nodes"][1]["wire"].as_str().unwrap();
    let original: Value = serde_json::from_str(wire).unwrap();
    for (index, changed) in [
        (1, Value::Null),
        (2, Value::Null),
        (3, Value::Null),
        (3, serde_json::json!([])),
        (4, Value::Null),
    ] {
        let mut bad = original.clone();
        bad[index] = changed;
        assert!(CausalMerkleSnapshot::decode_node(node.root.as_str(), &bad.to_string()).is_err());
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn admission_and_retained_deletion() {
    let original = values(&fixtures()["snapshots"][1]["versions"])[0].clone();
    let newer = CausalVersion::advance(
        1,
        &original.dot.author,
        1,
        std::slice::from_ref(&original),
        None,
    )
    .unwrap();
    let state = snapshot(&[original.clone(), newer.clone(), original.clone()]);
    assert_eq!(
        state.leaf(1).unwrap().versions.as_slice(),
        std::slice::from_ref(&newer)
    );
    CausalMerkleSnapshot::verify_leaf(state.root(), &state.leaf(1).unwrap()).unwrap();
    let mut changed = original.clone();
    changed.payload = None;
    assert!(CausalMerkleSnapshot::new(
        &[original.clone(), changed],
        CausalMerkleOptions::default()
    )
    .is_err());
    let mut reused = original.clone();
    reused.address = 2;
    assert!(
        CausalMerkleSnapshot::new(&[original.clone(), reused], CausalMerkleOptions::default())
            .is_err()
    );
    for options in [
        CausalMerkleOptions {
            max_versions: 0,
            ..Default::default()
        },
        CausalMerkleOptions {
            max_versions: 4097,
            ..Default::default()
        },
        CausalMerkleOptions {
            max_bytes: 0,
            ..Default::default()
        },
        CausalMerkleOptions {
            max_bytes: 8_388_609,
            ..Default::default()
        },
    ] {
        assert!(CausalMerkleSnapshot::new(&[], options).is_err());
    }
    assert!(CausalMerkleSnapshot::new(
        &[original.clone(), original.clone()],
        CausalMerkleOptions {
            max_versions: 1,
            ..Default::default()
        }
    )
    .is_err());
    assert!(CausalMerkleSnapshot::new(
        std::slice::from_ref(&original),
        CausalMerkleOptions {
            max_bytes: 1,
            ..Default::default()
        }
    )
    .is_err());
    assert!(
        CausalMerkleSnapshot::new(&vec![original.clone(); 65], CausalMerkleOptions::default())
            .is_err()
    );
    assert!(state.leaf(0).is_err());
    assert!(state.node("x").is_err());
    assert!(state.node(&"0".repeat(16)).is_err());
    let mut leaf = state.leaf(1).unwrap();
    leaf.versions = vec![original, newer];
    assert!(CausalMerkleSnapshot::verify_leaf(state.root(), &leaf).is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn all_snapshot_pairs_match_direct_causal_union() {
    let cases = fixtures();
    let cases = cases["snapshots"].as_array().unwrap();
    for source_case in cases {
        for destination_case in cases {
            let mut source = snapshot(&values(&source_case["versions"]));
            let store = Store::default();
            let mut destination = owner(store.clone());
            destination
                .receive(&values(&destination_case["versions"]))
                .unwrap();
            let progress =
                pull_causal(&mut source, &mut destination, CausalPullOptions::default()).unwrap();
            assert_eq!(progress.root, source.root());
            assert!(progress.complete);
            let mut groups: BTreeMap<u64, Vec<CausalVersion>> = BTreeMap::new();
            for value in values(&source_case["versions"])
                .into_iter()
                .chain(values(&destination_case["versions"]))
            {
                groups.entry(value.address).or_default().push(value);
            }
            let expected: Vec<_> = groups
                .values()
                .flat_map(|values| CausalVersion::merge(values, &[]).unwrap())
                .collect();
            assert_eq!(
                wires(&destination.snapshot().unwrap().versions),
                wires(&expected)
            );
            let calls = store.0.borrow().calls;
            let retry =
                pull_causal(&mut source, &mut destination, CausalPullOptions::default()).unwrap();
            assert!(retry.complete);
            assert_eq!(retry.changed_leaves, 0);
            assert_eq!(store.0.borrow().calls, calls);
            if source_case["root"] == destination_case["root"]
                || source_case["versions"].as_array().unwrap().is_empty()
            {
                assert_eq!((progress.node_requests, progress.leaf_requests), (0, 0));
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bounded_and_interrupted_attempts_resume() {
    let source = Rc::new(snapshot(&values(&fixtures()["snapshots"][5]["versions"])));
    let store = Store::default();
    let mut destination = owner(store.clone());
    let mut remote = Remote::new(source.clone());
    let short = pull_causal(
        &mut remote,
        &mut destination,
        CausalPullOptions {
            max_node_requests: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!short.complete);
    assert_eq!(
        (
            short.node_requests,
            short.leaf_requests,
            store.0.borrow().calls
        ),
        (1, 0, 0)
    );
    assert!(
        !pull_causal(
            &mut remote,
            &mut destination,
            CausalPullOptions {
                max_version_bytes: 1,
                ..Default::default()
            }
        )
        .unwrap()
        .complete
    );
    assert_eq!(store.0.borrow().calls, 0);
    let first = pull_causal(
        &mut remote,
        &mut destination,
        CausalPullOptions {
            max_leaf_requests: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!first.complete);
    assert_eq!(first.changed_leaves, 1);
    let mut reads = 0;
    let captured = source.clone();
    remote.on_leaf = Some(Box::new(move |key| {
        reads += 1;
        if reads == 2 {
            Err(LinkError::StorageError("interrupted".into()))
        } else {
            captured.leaf(key)
        }
    }));
    assert!(pull_causal(&mut remote, &mut destination, CausalPullOptions::default()).is_err());
    assert_eq!(destination.snapshot().unwrap().versions.len(), 2);
    remote.on_leaf = None;
    assert!(
        pull_causal(&mut remote, &mut destination, CausalPullOptions::default())
            .unwrap()
            .complete
    );
    assert_eq!(
        wires(&destination.snapshot().unwrap().versions),
        wires(&values(&fixtures()["snapshots"][5]["versions"]))
    );
    for options in [
        CausalPullOptions {
            max_node_requests: 0,
            ..Default::default()
        },
        CausalPullOptions {
            max_node_requests: 65537,
            ..Default::default()
        },
        CausalPullOptions {
            max_leaf_requests: 0,
            ..Default::default()
        },
        CausalPullOptions {
            max_leaf_requests: 4097,
            ..Default::default()
        },
        CausalPullOptions {
            max_version_bytes: 0,
            ..Default::default()
        },
        CausalPullOptions {
            max_version_bytes: 8_388_609,
            ..Default::default()
        },
    ] {
        assert!(pull_causal(&mut remote, &mut destination, options).is_err());
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn concurrent_writes_and_lost_publication_replies() {
    let source = Rc::new(snapshot(&values(&fixtures()["snapshots"][1]["versions"])));
    let store = Store::default();
    let mut destination = owner(store.clone());
    let mut sibling = owner(store.clone());
    let mut changed = false;
    let captured = source.clone();
    let mut remote = Remote::new(source.clone());
    remote.on_node = Some(Box::new(move |path| {
        if !changed {
            changed = true;
            sibling.write(
                &token(1),
                1,
                &[],
                Some(CausalPayload {
                    source: 0,
                    target: 0,
                    name: Some("concurrent".into()),
                }),
            )?;
        }
        captured.node(path)
    }));
    pull_causal(&mut remote, &mut destination, CausalPullOptions::default()).unwrap();
    let versions = destination.read(1).unwrap();
    assert_eq!(versions.len(), 2);
    assert!(versions
        .iter()
        .any(|v| v.payload.as_ref().and_then(|v| v.name.as_deref()) == Some("concurrent")));
    for fault in [1, 2] {
        let failed = Store::default();
        failed.0.borrow_mut().fault = fault;
        let mut remote = Remote::new(source.clone());
        assert!(pull_causal(
            &mut remote,
            &mut owner(failed.clone()),
            CausalPullOptions::default()
        )
        .is_err());
        let calls = failed.0.borrow().calls;
        let mut reopened = owner(failed.clone());
        assert!(
            pull_causal(&mut remote, &mut reopened, CausalPullOptions::default())
                .unwrap()
                .complete
        );
        assert_eq!(failed.0.borrow().calls, calls + usize::from(fault == 1));
        reopened.write(&token(1), 2, &[], None).unwrap();
        assert_eq!(reopened.snapshot().unwrap().last_counter, 1);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn requested_root_path_and_leaf_precede_publication() {
    let source = Rc::new(snapshot(&values(&fixtures()["snapshots"][1]["versions"])));
    let other = Rc::new(snapshot(&values(&fixtures()["snapshots"][4]["versions"])));
    for control in 0..5 {
        let captured = source.clone();
        let changed = other.clone();
        let mut remote = Remote::new(source.clone());
        match control {
            0 => remote.on_node = Some(Box::new(move |_| changed.node(""))),
            1 => remote.on_node = Some(Box::new(move |_| captured.node("0"))),
            2 => remote.on_leaf = Some(Box::new(move |_| captured.leaf(2))),
            3 => remote.on_leaf = Some(Box::new(move |key| changed.leaf(key))),
            _ => {
                remote.on_leaf = Some(Box::new(move |key| {
                    let mut leaf = captured.leaf(key)?;
                    leaf.versions[0].payload = None;
                    Ok(leaf)
                }))
            }
        }
        let store = Store::default();
        assert!(pull_causal(
            &mut remote,
            &mut owner(store.clone()),
            CausalPullOptions::default()
        )
        .is_err());
        assert_eq!(store.0.borrow().calls, 0);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bounded_continuation_reaches_all_addresses_with_concurrent_first_leaf() {
    let source = Rc::new(snapshot(&values(&fixtures()["snapshots"][5]["versions"])));
    let mut remote = Remote::new(source.clone());
    let mut destination = owner(Store::default());
    let concurrent = destination
        .write(
            &token(1),
            1,
            &[],
            Some(CausalPayload {
                source: 0,
                target: 0,
                name: Some("local".into()),
            }),
        )
        .unwrap();
    let mut progress = pull_causal(
        &mut remote,
        &mut destination,
        CausalPullOptions {
            max_node_requests: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let cursor = progress.continuation.clone().unwrap();
    assert_eq!(cursor.root(), source.root());
    let mut other = Remote::new(Rc::new(snapshot(&[])));
    assert!(pull_causal(
        &mut other,
        &mut destination,
        CausalPullOptions {
            continuation: Some(cursor.clone()),
            ..Default::default()
        }
    )
    .is_err());
    let foreign_store = Store::default();
    let mut foreign = CausalReplica::new(
        foreign_store.clone(),
        "replica",
        "0000000000000000000000000000000f",
        CausalReplicaOptions::default(),
    )
    .unwrap();
    assert!(pull_causal(
        &mut remote,
        &mut foreign,
        CausalPullOptions {
            continuation: Some(cursor.clone()),
            ..Default::default()
        }
    )
    .is_err());
    assert_eq!(foreign_store.0.borrow().calls, 0);
    for _ in 0..128 {
        progress = pull_causal(
            &mut remote,
            &mut destination,
            CausalPullOptions {
                max_node_requests: 1,
                max_leaf_requests: 1,
                continuation: progress.continuation,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(progress.node_requests <= 1 && progress.leaf_requests <= 1);
        if progress.complete {
            break;
        }
    }
    assert!(progress.complete);
    assert!(progress.continuation.is_none());
    let mut expected = values(&fixtures()["snapshots"][5]["versions"]);
    expected.push(concurrent);
    assert_eq!(
        wires(&destination.snapshot().unwrap().versions),
        wires(&expected)
    );
    assert!(
        pull_causal(
            &mut remote,
            &mut destination,
            CausalPullOptions {
                continuation: Some(cursor.clone()),
                ..Default::default()
            }
        )
        .unwrap()
        .complete
    );
    let mut identical = owner(Store::default());
    identical
        .receive(&values(&fixtures()["snapshots"][5]["versions"]))
        .unwrap();
    assert!(pull_causal(
        &mut remote,
        &mut identical,
        CausalPullOptions {
            continuation: Some(cursor),
            ..Default::default()
        }
    )
    .unwrap()
    .continuation
    .is_none());
    let mut leaf_limited = owner(Store::default());
    leaf_limited
        .write(
            &token(1),
            1,
            &[],
            Some(CausalPayload {
                source: 0,
                target: 0,
                name: Some("local".into()),
            }),
        )
        .unwrap();
    let mut leaf_cursor = None;
    let mut complete = false;
    for _ in 0..8 {
        let next = pull_causal(
            &mut remote,
            &mut leaf_limited,
            CausalPullOptions {
                max_leaf_requests: 1,
                continuation: leaf_cursor,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(next.leaf_requests <= 1);
        if next.complete {
            complete = true;
            break;
        }
        leaf_cursor = next.continuation;
    }
    assert!(complete);
    assert_eq!(
        wires(&leaf_limited.snapshot().unwrap().versions),
        wires(&expected)
    );
}
