//! WebAssembly tests for the browser-facing Rust wrapper.

#![cfg(target_arch = "wasm32")]

use clink_wasm::Clink;
use serde_json::Value;
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn creates_a_clink_instance() {
    let _clink = Clink::new();
}

#[wasm_bindgen_test]
fn exposes_versions() {
    assert_eq!(Clink::version(), "2.3.0");
    assert!(Clink::rust_core_version().starts_with("clink "));
}

#[wasm_bindgen_test]
fn executes_lino_queries_with_the_rust_core() {
    let mut clink = Clink::new();
    let raw = clink.execute(
        "() ((child: father mother))",
        r#"{"changes":true,"after":true}"#,
    );
    let parsed: Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(parsed["success"], true);
    assert!(parsed["output"].as_str().unwrap().contains("child"));
    assert_eq!(parsed["links"].as_array().unwrap().len(), 3);
}

#[wasm_bindgen_test]
fn reports_invalid_options() {
    let mut clink = Clink::new();
    let raw = clink.execute("() ((1 1))", "invalid json");
    let parsed: Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(parsed["success"], false);
    assert!(parsed["error"]
        .as_str()
        .unwrap()
        .contains("Invalid options JSON"));
}

#[wasm_bindgen_test]
fn javascript_wasm_api_round_trips_create_update_delete_and_recreate() {
    let mut clink = Clink::new();

    let created = execute_json(&mut clink, "(() ((1: 1 1)))");
    assert_eq!(created["success"], true);
    assert_link(&created, 1, 1, 1);

    let updated = execute_json(&mut clink, "(((1: 1 1)) ((1: 2 2)))");
    assert_eq!(updated["success"], true);
    assert_link(&updated, 1, 2, 2);

    let reverted = execute_json(&mut clink, "(((1: 2 2)) ((1: 1 1)))");
    assert_eq!(reverted["success"], true);
    assert_link(&reverted, 1, 1, 1);

    let deleted = execute_json(&mut clink, "((1: 1 1)) ()");
    assert_eq!(deleted["success"], true);
    assert_link_missing(&deleted, 1);

    let recreated = execute_json(&mut clink, "(() ((1: 1 1)))");
    assert_eq!(recreated["success"], true);
    assert_link(&recreated, 1, 1, 1);
}

fn execute_json(clink: &mut Clink, query: &str) -> Value {
    let raw = clink.execute(
        query,
        r#"{"changes":true,"after":true,"autoCreateMissingReferences":true}"#,
    );
    serde_json::from_str(&raw).unwrap()
}

fn assert_link(parsed: &Value, id: u64, source: u64, target: u64) {
    let links = parsed["links"].as_array().unwrap();
    let link = links
        .iter()
        .find(|link| link["id"] == id)
        .expect("link should exist");
    assert_eq!(link["source"], source);
    assert_eq!(link["target"], target);
}

fn assert_link_missing(parsed: &Value, id: u64) {
    let links = parsed["links"].as_array().unwrap();
    assert!(!links.iter().any(|link| link["id"] == id));
}
