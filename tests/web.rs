//! Test suite for the Web and headless browsers.

#![cfg(target_arch = "wasm32")]

extern crate wasm_bindgen_test;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

use clink_wasm::*;

#[wasm_bindgen_test]
fn test_clink_creation() {
    let clink = Clink::new();
    // Test that we can create a Clink instance
    assert!(true); // Basic smoke test
}

#[wasm_bindgen_test]
fn test_version() {
    let version = Clink::version();
    assert!(!version.is_empty());
    assert!(version.contains("2.2.2"));
}

#[wasm_bindgen_test]
fn test_wasm_functionality() {
    let result = Clink::test();
    assert_eq!(result, true);
}

#[wasm_bindgen_test]
fn test_basic_query_execution() {
    let mut clink = Clink::new();
    let options = r#"{"db": "memory", "trace": false, "structure": null, "before": false, "changes": true, "after": true}"#;
    
    // Test creation query
    let result = clink.execute("() ((1 1))", options);
    assert!(!result.is_empty());
    
    // Parse the result
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], true);
}

#[wasm_bindgen_test]
fn test_invalid_query() {
    let mut clink = Clink::new();
    let options = r#"{"db": "memory", "trace": false, "structure": null, "before": false, "changes": false, "after": false}"#;
    
    // Test invalid query
    let result = clink.execute("invalid query format", options);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], false);
    assert!(parsed["error"].is_string());
}

#[wasm_bindgen_test]
fn test_invalid_options() {
    let mut clink = Clink::new();
    
    // Test with invalid JSON options
    let result = clink.execute("() ((1 1))", "invalid json");
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], false);
    assert!(parsed["error"].as_str().unwrap().contains("Invalid options JSON"));
}