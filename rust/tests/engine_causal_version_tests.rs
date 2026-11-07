#![cfg(feature = "causal")]
use link_cli::engine::causal::{CausalDot, CausalPayload, CausalVersion};
use serde_json::Value;
fn vectors() -> Value {
    serde_json::from_str(include_str!("../../docs/protocol/causal-version-v1.json")).unwrap()
}
fn version(value: &Value) -> CausalVersion {
    CausalVersion::decode(&value.to_string()).unwrap()
}
fn versions(value: &Value) -> Vec<CausalVersion> {
    value.as_array().unwrap().iter().map(version).collect()
}
fn wires(values: &[CausalVersion]) -> Vec<String> {
    values.iter().map(|v| v.encode().unwrap()).collect()
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_causal_vectors_preserve_canonical_bytes_and_reject_malformed_inputs() {
    for fixture in vectors()["cases"].as_array().unwrap() {
        let wire = fixture["record"].as_str().unwrap();
        let result = CausalVersion::decode(wire);
        if fixture["valid"] == true {
            assert_eq!(
                result
                    .unwrap_or_else(|e| panic!("{}: {e}", fixture["name"]))
                    .encode()
                    .unwrap(),
                fixture["canonical"].as_str().unwrap()
            );
        } else {
            assert!(result.is_err(), "{}", fixture["name"]);
        }
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn causal_merge_retains_siblings_and_rejects_contradictions_before_pruning() {
    for fixture in vectors()["merges"].as_array().unwrap() {
        let left = versions(&fixture["left"]);
        let right = versions(&fixture["right"]);
        let before = (left.clone(), right.clone());
        let result = CausalVersion::merge(&left, &right);
        if fixture["valid"] == false {
            assert!(result.is_err(), "{}", fixture["name"]);
        } else {
            let expected = versions(&fixture["expected"]);
            assert_eq!(
                wires(&result.unwrap()),
                wires(&expected),
                "{}",
                fixture["name"]
            );
            assert_eq!(
                wires(&CausalVersion::merge(&right, &left).unwrap()),
                wires(&expected)
            );
        }
        assert_eq!((left, right), before);
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_delivery_orders_converge_and_merge_laws_hold() {
    let events = versions(&vectors()["events"]);
    let mut orders = 0;
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let mut unique = vec![a, b, c, d];
                    unique.sort();
                    unique.dedup();
                    if unique.len() != 4 {
                        continue;
                    }
                    let mut state = Vec::new();
                    for index in [a, b, c, d] {
                        state = CausalVersion::merge(&state, &[events[index].clone()]).unwrap();
                    }
                    assert_eq!(state, vec![events[3].clone()]);
                    orders += 1;
                }
            }
        }
    }
    assert_eq!(orders, 24);
    let states = [
        vec![],
        vec![events[0].clone()],
        vec![events[1].clone()],
        vec![events[2].clone()],
        vec![events[3].clone()],
        CausalVersion::merge(&[events[1].clone()], &[events[2].clone()]).unwrap(),
    ];
    for a in &states {
        for b in &states {
            for c in &states {
                assert_eq!(CausalVersion::merge(a, a).unwrap(), *a);
                assert_eq!(
                    CausalVersion::merge(a, b).unwrap(),
                    CausalVersion::merge(b, a).unwrap()
                );
                assert_eq!(
                    CausalVersion::merge(&CausalVersion::merge(a, b).unwrap(), c).unwrap(),
                    CausalVersion::merge(a, &CausalVersion::merge(b, c).unwrap()).unwrap()
                );
            }
        }
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn advancement_joins_observed_clocks_and_rejects_exhaustion_without_mutation() {
    let vectors = vectors();
    for fixture in vectors["advances"].as_array().unwrap() {
        let observed = versions(&fixture["observed"]);
        let before = observed.clone();
        let input = &fixture["payload"];
        let payload = if input.is_null() {
            None
        } else {
            let name = input[2].as_str().map(|text| {
                let bytes = (0..text.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
                    .collect();
                String::from_utf8(bytes).unwrap()
            });
            Some(CausalPayload {
                source: input[0].as_str().unwrap().parse().unwrap(),
                target: input[1].as_str().unwrap().parse().unwrap(),
                name,
            })
        };
        let result = CausalVersion::advance(
            fixture["address"].as_str().unwrap().parse().unwrap(),
            fixture["author"].as_str().unwrap(),
            fixture["lastCounter"].as_str().unwrap().parse().unwrap(),
            &observed,
            payload,
        );
        if fixture["valid"] == false {
            assert!(result.is_err(), "{}", fixture["name"]);
        } else {
            assert_eq!(
                result.unwrap(),
                version(&fixture["expected"]),
                "{}",
                fixture["name"]
            );
        }
        assert_eq!(observed, before);
    }
    let events = versions(&vectors["events"]);
    let author = vectors["authors"][0].as_str().unwrap();
    let siblings = CausalVersion::merge(&[events[1].clone()], &[events[2].clone()]).unwrap();
    let next = CausalVersion::advance(
        1,
        author,
        2,
        &siblings,
        Some(CausalPayload {
            source: 4,
            target: 5,
            name: Some("resolved".into()),
        }),
    )
    .unwrap();
    assert_eq!(next, events[3]);
    assert_eq!(
        CausalVersion::advance(1, author, 0, &[], None)
            .unwrap()
            .dot
            .counter,
        1
    );
    assert_eq!(
        CausalVersion::advance(1, author, 0, &[events[1].clone()], None)
            .unwrap()
            .dot
            .counter,
        3
    );
    assert!(CausalVersion::advance(1, author, u64::MAX, &siblings, None).is_err());
    assert!(CausalVersion::advance(2, author, 0, &siblings, None).is_err());
    assert_eq!(
        CausalVersion::merge(&[next], &siblings).unwrap(),
        vec![events[3].clone()]
    );
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn public_state_and_surviving_sibling_bounds_are_validated_and_results_are_owned() {
    let initial = version(&vectors()["events"][0]);
    let mut invalid = initial.clone();
    invalid.address = 0;
    assert!(invalid.encode().is_err());
    invalid = initial.clone();
    invalid.clock.clear();
    assert!(invalid.encode().is_err());
    invalid = initial.clone();
    invalid.dot.counter = 0;
    assert!(invalid.encode().is_err());
    invalid = initial.clone();
    invalid.clock[0].counter = 2;
    assert!(invalid.encode().is_err());
    invalid = initial.clone();
    invalid.dot.author = "invalid".into();
    assert!(invalid.encode().is_err());
    invalid = initial.clone();
    invalid.payload.as_mut().unwrap().name = Some("雪".repeat(1366));
    assert!(invalid.encode().is_err());
    assert!(CausalVersion::merge(&vec![initial.clone(); 65], &[]).is_err());
    let siblings: Vec<_> = (1..=65)
        .map(|n| {
            let dot = CausalDot {
                author: format!("{n:032x}"),
                counter: 1,
            };
            CausalVersion {
                address: 1,
                dot: dot.clone(),
                clock: vec![dot],
                payload: None,
            }
        })
        .collect();
    assert!(CausalVersion::merge(&siblings[..64], &siblings[64..]).is_err());
    let mut result = CausalVersion::merge(std::slice::from_ref(&initial), &[]).unwrap();
    result[0].clock[0].counter = 9;
    assert_eq!(initial.clock[0].counter, 1);
}
