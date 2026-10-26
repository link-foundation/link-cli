#![cfg(feature = "etcd")]
use base64::{engine::general_purpose::STANDARD, Engine as _};
use link_cli::engine::{
    cluster::{AtomicJournalStore, JournalVersion},
    etcd::EtcdJournalStore,
};
use serde_json::json;
#[path = "support/etcd_peer.rs"]
mod wire;
use wire::{peer, peer_raw, peer_raw_headers};

#[test]
fn malformed_snapshot_shapes_and_payloads_are_rejected_on_the_wire() {
    let key = STANDARD.encode("/test/a");
    let valid = json!({"key":key,"mod_revision":"1","value":STANDARD.encode("[]")});
    let ranges = [
        json!(false),
        json!({"more":true}),
        json!({"more":null}),
        json!({"kvs":{}}),
        json!({"kvs":[valid.clone(), valid.clone()]}),
        json!({"kvs":[{"key":"wrong","mod_revision":"1","value":STANDARD.encode("[]")}]}),
        json!({"kvs":[{"key":key,"mod_revision":1,"value":STANDARD.encode("[]")}]}),
        json!({"kvs":[{"key":key,"mod_revision":"1"}]}),
        json!({"kvs":[{"key":key,"mod_revision":"1","value":17}]}),
        json!({"kvs":[{"key":key,"mod_revision":"1","value":"!"}]}),
        json!({"kvs":[{"key":key,"mod_revision":"1","value":"W10=\n"}]}),
        json!({"kvs":[{"key":key,"mod_revision":"1","value":STANDARD.encode("not json")}]}),
        json!({"kvs":[{"key":key,"mod_revision":"1","value":STANDARD.encode("null")}]}),
        json!({"kvs":[{"key":key,"mod_revision":"1","value":STANDARD.encode("[null]")}]}),
    ];
    let mut replies: Vec<_> = ranges
        .into_iter()
        .map(|range| Some(json!({"succeeded":true,"responses":[{"response_range":range}]})))
        .collect();
    replies.extend([
        Some(json!({"succeeded":false,"responses":[{"response_range":{}}]})),
        Some(json!({"succeeded":true,"responses":[]})),
        Some(json!({"succeeded":true,"responses":[{}]})),
        Some(json!({"succeeded":true,"responses":null})),
    ]);
    let count = replies.len();
    let (endpoint, handle) = peer(replies);
    let mut store = EtcdJournalStore::new(vec![endpoint], "/test/".into()).unwrap();
    for index in 0..count {
        assert!(store.read(&["a".into()]).is_err(), "reply {index}");
    }
    assert_eq!(handle.join().unwrap().len(), count);
}

#[test]
fn cas_reply_generations_are_strict_and_success_is_reported() {
    let mut replies: Vec<_> = [
        "",
        "0",
        "01",
        "-1",
        "+1",
        "1 ",
        "9223372036854775808",
        "18446744073709551616",
    ]
    .into_iter()
    .map(|revision| Some(json!({"header":{"revision":revision},"succeeded":true})))
    .collect();
    replies.push(Some(json!({"header":{"revision":"1"},"succeeded":true})));
    let count = replies.len();
    let (endpoint, handle) = peer(replies);
    let mut store = EtcdJournalStore::new(vec![endpoint], "/test/".into()).unwrap();
    assert!(store.supports_durable_flush());
    for _ in 0..count - 1 {
        assert!(store
            .compare_exchange(&["a".into()], &[JournalVersion::default()], &[None])
            .is_err());
    }
    assert!(store
        .compare_exchange(&["a".into()], &[JournalVersion::default()], &[None])
        .unwrap());
    assert_eq!(handle.join().unwrap().len(), count);
}

#[test]
fn configured_headers_are_sent_and_invalid_headers_fail_locally() {
    let (endpoint, handle) = peer_raw_headers(
        vec![Some(
            json!({"succeeded":true,"responses":[{"response_range":{}}]}).to_string(),
        )],
        vec!["Authorization: fixture-token", "X-Tenant: fixture-tenant"],
    );
    let mut store = EtcdJournalStore::new(vec![endpoint], "/test/".into()).unwrap();
    store.headers = vec![
        ("Authorization".into(), "fixture-token".into()),
        ("X-Tenant".into(), "fixture-tenant".into()),
    ];
    assert_eq!(
        store.read(&["a".into()]).unwrap(),
        [JournalVersion::default()]
    );
    assert_eq!(handle.join().unwrap().len(), 1);
    // No peer accepts these requests; malformed headers fail before network I/O.
    for header in [
        ("bad name", "value"),
        ("X-Tenant", "value\r\nInjected: value"),
    ] {
        let mut store =
            EtcdJournalStore::new(vec!["http://127.0.0.1:1".into()], "/test/".into()).unwrap();
        store.headers = vec![(header.0.into(), header.1.into())];
        assert!(store.read(&["a".into()]).is_err());
    }
}

#[test]
fn invalid_configuration_and_json_fail_before_an_outcome_is_assumed() {
    for endpoint in [
        "relative",
        "http://localhost/?query=1",
        "http://[invalid",
        "file:///tmp/a",
    ] {
        assert!(EtcdJournalStore::new(vec![endpoint.into()], "/test/".into()).is_err());
    }
    for prefix in ["", "/", "without-slash"] {
        assert!(EtcdJournalStore::new(vec!["https://localhost".into()], prefix.into()).is_err());
    }
    let mut store =
        EtcdJournalStore::new(vec!["https://localhost".into()], "/test/".into()).unwrap();
    assert!(store.compare_exchange(&["a".into()], &[], &[None]).is_err());
    assert!(store
        .compare_exchange(&["a".into()], &[JournalVersion::default()], &[])
        .is_err());
    assert!(store
        .compare_exchange(
            &["a".into()],
            &[JournalVersion {
                generation: u64::MAX,
                entries: vec![]
            }],
            &[None]
        )
        .is_err());
    let (endpoint, handle) = peer_raw(vec![Some("{".into())]);
    let mut store = EtcdJournalStore::new(vec![endpoint], "/test/".into()).unwrap();
    assert!(store
        .read(&["a".into()])
        .unwrap_err()
        .to_string()
        .contains("invalid coordinator response"));
    handle.join().unwrap();
}
#[test]
fn etcd_snapshot_and_cas_check_every_selected_generation_and_accept_omitted_false() {
    let (endpoint, handle) = peer(vec![
        Some(json!({"succeeded":true,"responses":[
        {"response_range":{"kvs":[{"key":STANDARD.encode("/test/a"),"mod_revision":"7","value":STANDARD.encode("[\"record\"]")}]}},
        {"response_range":{}}]})),
        Some(json!({"header":{"revision":"9"}})),
    ]);
    let mut store = EtcdJournalStore::new(vec![endpoint], "/test/".into()).unwrap();
    let keys = vec!["a".into(), "b".into()];
    let snapshot = store.read(&keys).unwrap();
    assert_eq!(
        snapshot,
        vec![
            JournalVersion {
                generation: 7,
                entries: vec!["record".into()]
            },
            JournalVersion::default()
        ]
    );
    assert!(!store
        .compare_exchange(
            &keys,
            &snapshot,
            &[Some(vec!["record".into(), "new".into()]), None]
        )
        .unwrap());
    let requests = handle.join().unwrap();
    assert_eq!(requests[0]["success"].as_array().unwrap().len(), 2);
    assert_eq!(requests[1]["compare"][1]["target"], "MOD");
    assert_eq!(requests[1]["compare"][1]["mod_revision"], "0");
    assert_eq!(requests[1]["success"].as_array().unwrap().len(), 1);
}
#[test]
fn etcd_safe_read_failover_but_lost_mutation_response_is_not_replayed() {
    let (endpoint, handle) = peer(vec![
        Some(json!({"succeeded":true,"responses":[{"response_range":{}}]})),
        None,
    ]);
    // Consume one request then close without a reply. Keeping this endpoint
    // owned avoids a released port being reused by a parallel fixture.
    let (absent_url, unavailable) = peer(vec![None]);
    let mut store = EtcdJournalStore::new(vec![absent_url, endpoint], "/test/".into()).unwrap();
    let keys = vec!["a".into()];
    let snapshot = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &snapshot, &[Some(vec!["new".into()])])
        .is_err());
    assert_eq!(handle.join().unwrap().len(), 2);
    assert_eq!(unavailable.join().unwrap().len(), 1);
}
#[test]
fn etcd_invalid_wire_responses_fail_closed() {
    for generation in ["+1", "01", "0", "9223372036854775808"] {
        let (endpoint, handle) = peer(vec![Some(
            json!({"succeeded":true,"responses":[{"response_range":{"kvs":[{
            "key":STANDARD.encode("/test/a"),"mod_revision":generation,"value":STANDARD.encode("[]")}]}}]}),
        )]);
        let mut store = EtcdJournalStore::new(vec![endpoint], "/test/".into()).unwrap();
        assert!(store.read(&["a".into()]).is_err());
        handle.join().unwrap();
    }
    for reply in [
        json!({}),
        json!({"header":{"revision":"1"},"succeeded":"false"}),
    ] {
        let (endpoint, handle) = peer(vec![Some(reply)]);
        let mut store = EtcdJournalStore::new(vec![endpoint], "/test/".into()).unwrap();
        assert!(store
            .compare_exchange(&["a".into()], &[JournalVersion::default()], &[None])
            .is_err());
        handle.join().unwrap();
    }
}
#[test]
fn etcd_bounds_and_coordinates_are_checked() {
    assert!(EtcdJournalStore::new(vec![], "/test/".into()).is_err());
    assert!(EtcdJournalStore::new(vec!["ftp://localhost".into()], "/test/".into()).is_err());
    let mut store =
        EtcdJournalStore::new(vec!["http://127.0.0.1:1".into()], "/test/".into()).unwrap();
    assert!(store.read(&["a".into(), "a".into()]).is_err());
    store.max_request_bytes = 1;
    assert!(store
        .compare_exchange(&["a".into()], &[JournalVersion::default()], &[None])
        .unwrap_err()
        .to_string()
        .contains("byte limit"));
    let (endpoint, handle) = peer(vec![Some(
        json!({"succeeded":true,"responses":[{"response_range":{}}]}),
    )]);
    let mut store = EtcdJournalStore::new(vec![endpoint], "/test/".into()).unwrap();
    store.max_response_bytes = 1;
    assert!(store
        .read(&["a".into()])
        .unwrap_err()
        .to_string()
        .contains("byte limit"));
    handle.join().unwrap();
}
