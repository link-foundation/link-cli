#![cfg(feature = "etcd")]
// Reuse the finite wire peer, not the behavior under test.
#[path = "support/etcd_peer.rs"]
mod wire;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use link_cli::engine::{
    etcd::EtcdJournalStore,
    maintenance::{JournalMaintenanceStore, JournalScanOptions},
};
use serde_json::{json, Value};
fn vectors() -> Value {
    serde_json::from_str(include_str!(
        "../../docs/protocol/journal-maintenance-v1.json"
    ))
    .unwrap()
}
fn options(value: &Value) -> JournalScanOptions {
    let mut result = JournalScanOptions::default();
    if let Some(value) = value["maxKeys"].as_u64() {
        result.max_keys = value as usize;
    }
    if let Some(value) = value["maxBytes"].as_u64() {
        result.max_bytes = value as usize;
    }
    if let Some(value) = value["pageKeys"].as_u64() {
        result.page_keys = value as usize;
    }
    if let Some(value) = value["keysOnly"].as_bool() {
        result.keys_only = value;
    }
    result
}
#[test]
fn shared_namespace_scan_vectors() {
    for case in vectors()["scans"].as_array().unwrap() {
        let replies = case["replies"].as_array().unwrap();
        let (endpoint, handle) = wire::peer(replies.iter().cloned().map(Some).collect());
        let mut store =
            EtcdJournalStore::new(vec![endpoint], case["prefix"].as_str().unwrap().into()).unwrap();
        let result = store.scan_namespace(options(&case["options"]));
        if case["expected"].is_null() {
            assert!(result.is_err(), "{}", case["name"]);
        } else {
            let result = result.unwrap_or_else(|cause| panic!("{}: {cause}", case["name"]));
            let values: Vec<_> = result.into_iter().map(|entry| json!({"key":entry.key,"version":{"generation":entry.version.generation.to_string(),"entries":entry.version.entries}})).collect();
            assert_eq!(json!(values), case["expected"], "{}", case["name"]);
        }
        let calls = handle.join().unwrap();
        assert_eq!(calls.len(), replies.len());
        assert!(calls[0]["success"][0]["request_range"]
            .get("revision")
            .is_none());
        if calls.len() > 1 {
            assert_eq!(calls[1]["success"][0]["request_range"]["revision"], "10");
        }
    }
}
#[test]
fn shared_conditional_delete_vectors() {
    for case in vectors()["deletes"].as_array().unwrap() {
        let (endpoint, handle) = wire::peer(vec![Some(case["reply"].clone())]);
        let mut store = EtcdJournalStore::new(vec![endpoint], "/test/".into()).unwrap();
        let result = store.compare_delete("a", 7);
        match case["expected"].as_bool() {
            None => assert!(result.is_err(), "{}", case["name"]),
            Some(expected) => assert_eq!(result.unwrap(), expected, "{}", case["name"]),
        }
        let calls = handle.join().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0],
            json!({"compare":[{"key":STANDARD.encode("/test/a"),"target":"MOD","result":"EQUAL","mod_revision":"7"}],"success":[{"request_delete_range":{"key":STANDARD.encode("/test/a")}}]})
        );
    }
}
#[test]
fn namespace_options_fail_before_io() {
    let mut store =
        EtcdJournalStore::new(vec!["http://127.0.0.1:1".into()], "/test/".into()).unwrap();
    for options in [
        JournalScanOptions {
            max_keys: 0,
            ..Default::default()
        },
        JournalScanOptions {
            max_bytes: 0,
            ..Default::default()
        },
        JournalScanOptions {
            page_keys: 0,
            ..Default::default()
        },
        JournalScanOptions {
            page_keys: 257,
            ..Default::default()
        },
    ] {
        assert!(store.scan_namespace(options).is_err());
    }
    for generation in [0, i64::MAX as u64 + 1] {
        assert!(store.compare_delete("a", generation).is_err());
    }
    for key in ["", "/", "a/b"] {
        assert!(store.compare_delete(key, 7).is_err());
    }
}
#[test]
fn scan_failover_restarts_all_pages_and_mutations_are_never_retried() {
    let page = vectors()["scans"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "pinned pagination")
        .unwrap()["replies"][0]
        .clone();
    let (first, first_handle) = wire::peer(vec![Some(page), None]);
    let (second, second_handle) = wire::peer(vec![
        Some(vectors()["scans"][0]["replies"][0].clone()),
        None,
    ]);
    let mut store = EtcdJournalStore::new(vec![first, second], "/test/".into()).unwrap();
    assert!(store
        .scan_namespace(JournalScanOptions {
            page_keys: 2,
            ..Default::default()
        })
        .unwrap()
        .is_empty());
    assert!(store.compare_delete("a", 7).is_err());
    assert_eq!(first_handle.join().unwrap().len(), 2);
    let calls = second_handle.join().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(calls[0]["success"][0]["request_range"]
        .get("revision")
        .is_none());
    assert_eq!(
        calls[0]["success"][0]["request_range"]["key"],
        STANDARD.encode("/test/")
    );
}
