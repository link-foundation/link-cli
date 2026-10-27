#![cfg(all(target_arch = "wasm32", feature = "causal-sessions"))]
use clink_wasm::CausalLinksWorkspace;
use link_cli::engine::causal::{CausalPayload, CausalVersion};
use serde_json::{json, Value};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn workspace_keeps_full_u64_names_and_freezes_owned_final_edits() {
    let maximum = u64::MAX.to_string();
    let mut ws = CausalLinksWorkspace::new("[]", &maximum, &maximum, "reject", "{}").unwrap();
    assert_eq!(
        ws.request(&json!(["create", "0", "2"]).to_string())
            .unwrap(),
        json!(maximum).to_string()
    );
    ws.request(&json!(["set-name", maximum, "\u{feff}雪😀\0"]).to_string())
        .unwrap();
    let before: Value = serde_json::from_str(&ws.request("[\"rows\"]").unwrap()).unwrap();
    assert_eq!(before[0]["index"], maximum);
    assert!(ws.request("[\"create\",\"0\",\"0\"]").is_err());
    let edits: Value = serde_json::from_str(&ws.finish().unwrap()).unwrap();
    assert_eq!(edits[0]["address"], maximum);
    assert_eq!(edits[0]["payload"]["name"], "\u{feff}雪😀\0");
    assert!(edits[0]["observed"].as_array().unwrap().is_empty());
    assert!(ws.request(&json!(["delete", maximum]).to_string()).is_err());
    assert_eq!(
        serde_json::from_str::<Value>(&ws.request("[\"rows\"]").unwrap()).unwrap(),
        before
    );
    assert!(ws.finish().is_err());
}

#[wasm_bindgen_test]
fn ordinary_transport_reads_updates_deletes_and_selected_options() {
    let mut ws = CausalLinksWorkspace::new(
        "[]", "1", "2", "max-dot",
        r#"{"stateStructure":"shared","constraintValidation":"indexed","constraints":["unique-names"],"maxVersions":8,"maxBytes":1024,"trace":true}"#,
    ).unwrap();
    assert_eq!(ws.request(r#"["ensure","1"]"#).unwrap(), r#""1""#);
    assert_eq!(
        ws.request(r#"["update","1","2","0"]"#).unwrap(),
        r#"{"index":"1","source":"0","target":"0"}"#
    );
    ws.request(r#"["set-name","1",""]"#).unwrap();
    assert_eq!(ws.request(r#"["get-name","1"]"#).unwrap(), r#""""#);
    assert_eq!(ws.request(r#"["get-by-name",""]"#).unwrap(), r#""1""#);
    assert_eq!(
        ws.request(r#"["get","1"]"#).unwrap(),
        r#"{"index":"1","source":"2","target":"0"}"#
    );
    assert_eq!(ws.request(r#"["get","2"]"#).unwrap(), "null");
    assert_eq!(ws.request(r#"["get-by-name","missing"]"#).unwrap(), "null");
    ws.request(r#"["set-name","1",null]"#).unwrap();
    assert_eq!(ws.request(r#"["get-name","1"]"#).unwrap(), "null");
    ws.request(r#"["create","1","1"]"#).unwrap();
    assert_eq!(
        ws.request(r#"["delete","2"]"#).unwrap(),
        r#"{"index":"2","source":"1","target":"1"}"#
    );
    ws.request(r#"["set-name","1","kept"]"#).unwrap();
    let edits: Value = serde_json::from_str(&ws.finish().unwrap()).unwrap();
    assert_eq!(edits.as_array().unwrap().len(), 1);
    assert_eq!(ws.request(r#"["get-name","1"]"#).unwrap(), r#""kept""#);
    assert_eq!(ws.request(r#"["get-by-name","kept"]"#).unwrap(), r#""1""#);
    assert_eq!(
        ws.request(r#"["get","1"]"#).unwrap(),
        r#"{"index":"1","source":"2","target":"0"}"#
    );

    // Preparation must encode every retained observation, including its full
    // clock and name, rather than only covering fresh rows without observers.
    let retained = CausalVersion::advance(
        1,
        "00000000000000000000000000000001",
        0,
        &[],
        Some(CausalPayload {
            source: 1,
            target: 1,
            name: Some("old\u{feff}雪😀\0".into()),
        }),
    )
    .unwrap()
    .encode()
    .unwrap();
    let mut seeded =
        CausalLinksWorkspace::new(&json!([retained]).to_string(), "32", "33", "reject", "{}")
            .unwrap();
    seeded.request(r#"["set-name","1","new"]"#).unwrap();
    let edits: Value = serde_json::from_str(&seeded.finish().unwrap()).unwrap();
    assert_eq!(edits[0]["observed"], json!([retained]));
    assert_eq!(edits[0]["payload"]["name"], "new");
}

#[wasm_bindgen_test]
fn malformed_transport_and_configuration_cannot_mutate_rows() {
    for configuration in [
        "[]",
        "null",
        "bad",
        r#"{"isolation":"snapshot"}"#,
        r#"{"maxVersions":"1"}"#,
        r#"{"maxBytes":-1}"#,
        r#"{"maxVersions":4294967296}"#,
        r#"{"maxBytes":4294967296}"#,
        r#"{"maxVersions":0}"#,
        r#"{"stateStructure":"unknown"}"#,
        r#"{"constraintValidation":"unknown"}"#,
        r#"{"constraints":["unknown"]}"#,
        r#"{"trace":"yes"}"#,
    ] {
        assert!(CausalLinksWorkspace::new("[]", "1", "2", "reject", configuration).is_err());
    }
    for (versions, first, last, policy) in [
        ("bad", "1", "2", "reject"),
        ("[1]", "1", "2", "reject"),
        (r#"["bad"]"#, "1", "2", "reject"),
        ("[]", "01", "2", "reject"),
        ("[]", "1", "-1", "reject"),
        ("[]", "0", "2", "reject"),
        ("[]", "2", "1", "reject"),
        ("[]", "1", "2", "unknown"),
    ] {
        assert!(CausalLinksWorkspace::new(versions, first, last, policy, "{}").is_err());
    }
    let mut ws = CausalLinksWorkspace::new("[]", "1", "2", "reject", "{}").unwrap();
    for operation in [
        "bad",
        "{}",
        "[]",
        r#"["unknown"]"#,
        r#"["unknown","1"]"#,
        r#"["unknown","1","name"]"#,
        r#"["unknown","1","0","0"]"#,
        r#"["create",1,"2"]"#,
        r#"["create","01","2"]"#,
        r#"["create","18446744073709551616","2"]"#,
        r#"["get-by-name",null]"#,
        r#"["set-name","1",false]"#,
        r#"["set-name","1","missing"]"#,
        r#"["ensure","3"]"#,
        r#"["ensure","0"]"#,
        r#"["update","1","0","0"]"#,
        r#"["delete","1"]"#,
    ] {
        assert!(ws.request(operation).is_err(), "{operation}");
        assert_eq!(ws.request(r#"["rows"]"#).unwrap(), "[]");
    }
    assert!(ws.request(&" ".repeat(1_048_577)).is_err());
    assert!(CausalLinksWorkspace::new(&" ".repeat(16_785_411), "1", "2", "reject", "{}").is_err());
    assert_eq!(ws.finish().unwrap(), "[]");
}
