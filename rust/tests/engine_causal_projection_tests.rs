#![cfg(feature = "causal-projection")]
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_projection::{CausalConflictPolicy, CausalProjection, CausalProjectionOptions},
};
use serde_json::Value;

fn author(value: u64) -> String {
    format!("{value:032x}")
}
fn payload(name: Option<&str>) -> CausalPayload {
    CausalPayload {
        source: 0,
        target: 2,
        name: name.map(str::to_owned),
    }
}
fn wires(values: &[CausalVersion]) -> Vec<String> {
    values.iter().map(|v| v.encode().unwrap()).collect()
}
fn projection(values: &[CausalVersion], policy: CausalConflictPolicy) -> CausalProjection {
    CausalProjection::new(values, policy, CausalProjectionOptions::default()).unwrap()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_projection_vectors_retain_complete_evidence_before_visibility() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../docs/protocol/causal-projection-v1.json"
    ))
    .unwrap();
    let cases = fixtures["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 132);
    for fixture in cases {
        let values: Vec<_> = fixture["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| CausalVersion::decode(&v.to_string()).unwrap())
            .collect();
        let policy = if fixture["policy"] == "reject" {
            CausalConflictPolicy::Reject
        } else {
            CausalConflictPolicy::MaxDot
        };
        for input in [values.clone(), values.iter().rev().cloned().collect()] {
            let before = input.clone();
            let result = CausalProjection::new(&input, policy, CausalProjectionOptions::default());
            if fixture["valid"] == false {
                assert!(
                    result
                        .err()
                        .unwrap()
                        .to_string()
                        .contains(fixture["error"].as_str().unwrap()),
                    "{}",
                    fixture["name"]
                );
            } else {
                let result = result.unwrap();
                for (field, output) in [
                    ("retained", result.versions()),
                    ("selected", result.selected()),
                ] {
                    assert_eq!(
                        wires(&output),
                        fixture[field]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(Value::to_string)
                            .collect::<Vec<_>>(),
                        "{}",
                        fixture["name"]
                    );
                }
                let rows = result.rows();
                let expected = fixture["rows"].as_array().unwrap();
                assert_eq!(rows.len(), expected.len());
                for (row, expected) in rows.iter().zip(expected) {
                    assert_eq!(row.link.index.to_string(), expected[0].as_str().unwrap());
                    assert_eq!(row.link.source.to_string(), expected[1].as_str().unwrap());
                    assert_eq!(row.link.target.to_string(), expected[2].as_str().unwrap());
                    assert_eq!(row.name.as_deref(), expected[3].as_str());
                }
            }
            assert_eq!(input, before);
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn projection_owns_inputs_and_returned_clocks_with_losing_live_siblings() {
    let live = CausalVersion::advance(1, &author(1), 0, &[], Some(payload(Some("\u{feff}雪😀\0"))))
        .unwrap();
    let deleted = CausalVersion::advance(1, &author(2), 0, &[], None).unwrap();
    let mut input = vec![live, deleted];
    let expected = input.clone();
    let result = projection(&input, CausalConflictPolicy::MaxDot);
    input[0].clock[0].counter = 99;
    input[0].payload.as_mut().unwrap().name = Some("caller".into());
    input.clear();
    assert!(result.rows().is_empty());
    assert_eq!(result.versions(), expected);
    result.versions()[0].clock[0].counter = 99;
    result.selected()[0].clock[0].counter = 99;
    assert_eq!(result.versions(), expected);
    let visible = projection(
        &[CausalVersion::advance(2, &author(3), 0, &[], Some(payload(Some("")))).unwrap()],
        CausalConflictPolicy::Reject,
    );
    visible.rows()[0].name = Some("returned".into());
    assert_eq!(visible.rows()[0].name.as_deref(), Some(""));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn complete_input_bounds_precede_deduplication_and_dominance() {
    let value = CausalVersion::advance(1, &author(1), 0, &[], Some(payload(Some("row")))).unwrap();
    let policy = CausalConflictPolicy::Reject;
    for options in [
        CausalProjectionOptions {
            max_versions: 0,
            ..Default::default()
        },
        CausalProjectionOptions {
            max_versions: 4097,
            ..Default::default()
        },
        CausalProjectionOptions {
            max_bytes: 0,
            ..Default::default()
        },
        CausalProjectionOptions {
            max_bytes: 8_388_609,
            ..Default::default()
        },
    ] {
        assert!(CausalProjection::new(&[], policy, options).is_err());
    }
    let one = CausalProjectionOptions {
        max_versions: 1,
        ..Default::default()
    };
    assert!(CausalProjection::new(&[value.clone(), value.clone()], policy, one.clone()).is_err());
    assert_eq!(
        wires(&projection(&[value.clone(), value.clone()], policy).versions()),
        vec![value.encode().unwrap()]
    );
    let bytes = value.encode().unwrap().len();
    assert!(CausalProjection::new(
        std::slice::from_ref(&value),
        policy,
        CausalProjectionOptions {
            max_bytes: bytes - 1,
            ..Default::default()
        }
    )
    .is_err());
    assert_eq!(
        CausalProjection::new(
            std::slice::from_ref(&value),
            policy,
            CausalProjectionOptions {
                max_bytes: bytes,
                ..Default::default()
            }
        )
        .unwrap()
        .rows()
        .len(),
        1
    );
    assert!(CausalProjection::new(&vec![value.clone(); 65], policy, Default::default()).is_err());
    let concurrent: Vec<_> = (1..=64)
        .map(|i| CausalVersion::advance(1, &author(i), 0, &[], None).unwrap())
        .collect();
    assert_eq!(
        projection(&concurrent, CausalConflictPolicy::MaxDot)
            .versions()
            .len(),
        64
    );
    assert!(CausalProjection::new(
        &[],
        policy,
        CausalProjectionOptions {
            max_versions: 1,
            max_bytes: 1
        }
    )
    .unwrap()
    .rows()
    .is_empty());
    let newer =
        CausalVersion::advance(1, &author(1), 1, std::slice::from_ref(&value), None).unwrap();
    assert!(CausalProjection::new(&[value, newer], CausalConflictPolicy::MaxDot, one).is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn numeric_order_and_full_unsigned_values_preserve_null_names() {
    let input: Vec<_> = [u64::MAX, 10, 2]
        .iter()
        .enumerate()
        .map(|(index, address)| {
            CausalVersion::advance(
                *address,
                &author(index as u64 + 1),
                u64::MAX - 1,
                &[],
                Some(CausalPayload {
                    source: u64::MAX,
                    target: 0,
                    name: None,
                }),
            )
            .unwrap()
        })
        .collect();
    let result = projection(&input, CausalConflictPolicy::Reject);
    assert_eq!(
        result
            .rows()
            .iter()
            .map(|v| v.link.index)
            .collect::<Vec<_>>(),
        [2, 10, u64::MAX]
    );
    assert_eq!(result.rows()[0].link.source, u64::MAX);
    assert_eq!(result.rows()[0].name, None);
    assert_eq!(result.selected()[0].dot.counter, u64::MAX);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_admission_accepts_4096_versions_and_exactly_eight_mib() {
    fn make(index: u64, name: &str, source: u64) -> CausalVersion {
        CausalVersion::advance(
            index,
            &author(1),
            index - 1,
            &[],
            Some(CausalPayload {
                source,
                target: 0,
                name: Some(name.into()),
            }),
        )
        .unwrap()
    }
    let mut many: Vec<_> = (1..=4096).map(|index| make(index, "", 0)).collect();
    assert_eq!(
        projection(&many, CausalConflictPolicy::Reject).rows().len(),
        4096
    );
    many.push(make(4097, "", 0));
    assert!(
        CausalProjection::new(&many, CausalConflictPolicy::Reject, Default::default()).is_err()
    );
    const BUDGET: usize = 8_388_608;
    let mut wide = Vec::new();
    let mut bytes = 0;
    loop {
        let next = make(wide.len() as u64 + 1, &"a".repeat(4096), 0);
        let size = next.encode().unwrap().len();
        if bytes + size + 256 > BUDGET {
            break;
        }
        wide.push(next);
        bytes += size;
    }
    let mut last = make(wide.len() as u64 + 1, "", 0);
    let mut rest = BUDGET - bytes - last.encode().unwrap().len();
    if !rest.is_multiple_of(2) {
        last = make(wide.len() as u64 + 1, "", 10);
        rest = BUDGET - bytes - last.encode().unwrap().len();
    }
    assert!(rest <= 8192 && rest.is_multiple_of(2));
    last.payload.as_mut().unwrap().name = Some("a".repeat(rest / 2));
    wide.push(last);
    assert_eq!(
        wide.iter()
            .map(|v| v.encode().unwrap().len())
            .sum::<usize>(),
        BUDGET
    );
    assert_eq!(
        projection(&wide, CausalConflictPolicy::Reject).rows().len(),
        wide.len()
    );
    wide.push(make(wide.len() as u64 + 1, "", 0));
    assert!(
        CausalProjection::new(&wide, CausalConflictPolicy::Reject, Default::default()).is_err()
    );
}
