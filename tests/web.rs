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
