#![cfg(feature = "physical-lsm")]
use link_cli::engine::lsm::{LsmOptions, LsmWalState};
use serde_json::Value;
fn options(value: &Value) -> LsmOptions {
    let mut options = LsmOptions::default();
    for (key, target) in [
        ("maxKeys", &mut options.max_keys),
        ("maxBytes", &mut options.max_bytes),
        ("bufferKeys", &mut options.buffer_keys),
        ("bufferBytes", &mut options.buffer_bytes),
        ("maxRuns", &mut options.max_runs),
        ("maxWalRecords", &mut options.max_wal_records),
        ("maxWalBytes", &mut options.max_wal_bytes),
    ] {
        if let Some(value) = value[key].as_u64() {
            *target = value as usize;
        }
    }
    options
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_lsm_wal_vectors_and_checkpoint_roundtrip() {
    let vectors: Value =
        serde_json::from_str(include_str!("../../docs/protocol/native-lsm-wal-v1.json")).unwrap();
    for case in vectors["cases"].as_array().unwrap() {
        // A typed Rust log cannot return a non-string record; other languages
        // exercise that provider-shape control as well.
        let Some(records) = case["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let options = options(&case["options"]);
        let result = LsmWalState::recover(&records, options);
        if case["valid"] == true {
            let state = result.unwrap_or_else(|error| panic!("{}: {error}", case["name"]));
            let encoded = state.encode(options).unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&encoded).unwrap(),
                case["checkpoint"],
                "{}",
                case["name"]
            );
            assert_eq!(LsmWalState::recover(&[encoded], options).unwrap(), state);
        } else {
            assert!(result.is_err(), "{}", case["name"]);
        }
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checkpoint_encoding_revalidates_public_state_and_retains_owned_rows() {
    let records = vec![r#"["native-lsm1","1","0","0",[],[["a","1",["value"]]]]"#.to_owned()];
    let mut state = LsmWalState::recover(&records, LsmOptions::default()).unwrap();
    state.buffer[0].entries.as_mut().unwrap()[0] = "caller".into();
    assert_eq!(
        LsmWalState::recover(&records, LsmOptions::default())
            .unwrap()
            .buffer[0]
            .entries
            .as_ref()
            .unwrap()[0],
        "value"
    );
    state.generation = 0;
    assert!(state.encode(LsmOptions::default()).is_err());
}
