#![cfg(feature = "etcd")]
use base64::{engine::general_purpose::STANDARD, Engine as _};
use link_cli::engine::{
    cluster::{AtomicJournalStore, JournalVersion},
    etcd::EtcdJournalStore,
};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

// A bounded wire-level peer: no runtime, real sockets, finite deadlines, tiny bodies.
fn peer(replies: Vec<Option<Value>>) -> (String, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for reply in replies {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(cause)
                        if cause.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(cause) => panic!("bounded peer accept failed: {cause}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut byte = [0];
            while !bytes.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                bytes.push(byte[0]);
                assert!(bytes.len() < 8192);
            }
            let header = String::from_utf8(bytes).unwrap();
            let length: usize = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|text| text.trim().parse().unwrap())
                })
                .unwrap();
            assert!(length < 16384);
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            requests.push(serde_json::from_slice(&body).unwrap());
            if let Some(reply) = reply {
                let text = reply.to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len()).unwrap();
            }
        }
        requests
    });
    (endpoint, handle)
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
    // A closed local port is an immediate transport failure, not an external dependency.
    let absent = TcpListener::bind("127.0.0.1:0").unwrap();
    let absent_url = format!("http://{}", absent.local_addr().unwrap());
    drop(absent);
    let mut store = EtcdJournalStore::new(vec![absent_url, endpoint], "/test/".into()).unwrap();
    let keys = vec!["a".into()];
    let snapshot = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &snapshot, &[Some(vec!["new".into()])])
        .is_err());
    assert_eq!(handle.join().unwrap().len(), 2);
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
