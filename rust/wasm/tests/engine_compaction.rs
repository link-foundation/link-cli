//! Actual-WASM public compaction, capability and uncertain-outcome controls.
#![cfg(target_arch = "wasm32")]
use clink_wasm::StagedEngine;
use wasm_bindgen_test::wasm_bindgen_test;
#[wasm_bindgen_test]
fn volatile_memory_binding_rejects_optional_replacement_without_poisoning() {
    let engine = StagedEngine::new("{}", "[]").unwrap();
    assert!(engine.compact_journal().is_err());
    let mut tx = engine.begin().unwrap();
    assert!(!tx.request("(create: (0 0))").contains("error:"));
    assert_eq!(tx.commit().unwrap(), 1);
}

use serde_json::Value;
use wasm_bindgen::{prelude::*, JsCast};
#[wasm_bindgen(inline_js = r#"
export function compact_log(mode) {
    return {
        mode, entries: [], calls: 0, supportsDurableFlush: true,
        get supportsReplacement() {
            if (this.mode === 'cap-throw') throw 'capability failed';
            if (this.mode === 'cap-absent') return undefined;
            if (this.mode === 'cap-string') return 'true';
            if (this.mode === 'cap-number') return 1;
            return this.mode !== 'cap-false';
        },
        read() { return JSON.stringify(this.entries); },
        append(record) { this.entries.push(record); },
        flush() {},
        replace(records) {
            this.calls++;
            if (!Array.isArray(records) || records.some(x => typeof x !== 'string')) throw 'invalid record array';
            if (this.mode === 'before') throw 'before publication';
            if (this.mode === 'number-throw') throw 42;
            const changed = JSON.stringify(records) !== JSON.stringify(this.entries);
            this.entries = [...records];
            if (this.mode === 'after') throw 'after publication';
            if (this.mode === 'promise') return Promise.resolve(changed);
            if (this.mode === 'void') return undefined;
            if (this.mode === 'string') return 'true';
            return changed;
        }
    };
}
export function compact_mode(log, mode) { log.mode = mode; }
export function compact_calls(log) { return log.calls; }
"#)]
extern "C" {
    fn compact_log(mode: &str) -> JsValue;
    fn compact_mode(log: &JsValue, mode: &str);
    fn compact_calls(log: &JsValue) -> u32;
}
fn value(engine: &StagedEngine) -> Value {
    serde_json::from_str(&engine.snapshot(None).unwrap()).unwrap()
}
fn commit(engine: &StagedEngine) -> u64 {
    let mut tx = engine.begin().unwrap();
    assert!(!tx.request("(create: (0 0))").contains("error:"));
    tx.commit().unwrap()
}
#[wasm_bindgen_test]
fn capability_requires_exact_true_and_unsupported_engines_keep_committing() {
    for mode in [
        "cap-throw",
        "cap-absent",
        "cap-string",
        "cap-number",
        "cap-false",
    ] {
        let raw = compact_log(mode);
        let engine = StagedEngine::with_log("{}", raw.clone().unchecked_into()).unwrap();
        assert!(engine.compact_journal().unwrap_err().contains("support"));
        assert_eq!(compact_calls(&raw), 0);
        assert_eq!(commit(&engine), 1);
    }
}
#[wasm_bindgen_test]
fn external_log_gets_string_arrays_and_synchronous_boolean_results() {
    let raw = compact_log("ok");
    let engine = StagedEngine::with_log(
        r#"{"history":2,"requireDurableFlush":true}"#,
        raw.clone().unchecked_into(),
    )
    .unwrap();
    assert!(engine.compact_journal().unwrap());
    assert!(!engine.compact_journal().unwrap());
    for revision in 1..=4 {
        assert_eq!(commit(&engine), revision);
    }
    let before = value(&engine);
    let expected = engine.export_checkpoint().unwrap();
    assert!(engine.compact_journal().unwrap());
    assert!(!engine.compact_journal().unwrap());
    assert_eq!(engine.journal().unwrap(), expected);
    assert_eq!(value(&engine), before);
    let copy = StagedEngine::with_log(r#"{"history":2}"#, raw.unchecked_into()).unwrap();
    assert_eq!(value(&copy), before);
    assert_eq!(commit(&copy), 5);
}
#[wasm_bindgen_test]
fn thrown_and_invalid_acknowledgements_fence_binding_and_pending_sessions() {
    for mode in [
        "before",
        "after",
        "number-throw",
        "promise",
        "void",
        "string",
    ] {
        let raw = compact_log("ok");
        let engine = StagedEngine::with_log("{}", raw.clone().unchecked_into()).unwrap();
        assert_eq!(commit(&engine), 1);
        let pin = value(&engine);
        let mut pending = engine.begin().unwrap();
        assert!(!pending.request("(create: (0 0))").contains("error:"));
        compact_mode(&raw, mode);
        assert!(engine.compact_journal().is_err());
        let calls = compact_calls(&raw);
        assert!(engine.compact_journal().is_err());
        assert!(engine.snapshot(None).is_err());
        assert!(engine.begin().is_err());
        assert!(engine.export_checkpoint().is_err());
        assert!(pending.commit().is_err());
        assert_eq!(compact_calls(&raw), calls);
        drop(pending);
        drop(engine);
        compact_mode(&raw, "ok");
        let reopened = StagedEngine::with_log("{}", raw.unchecked_into()).unwrap();
        assert_eq!(value(&reopened), pin);
        assert_eq!(commit(&reopened), 2);
        assert!(reopened.compact_journal().unwrap());
        assert!(!reopened.compact_journal().unwrap());
    }
}
