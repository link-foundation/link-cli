//! Shared golden fixtures (issue #111): `parity/golden-fixtures.json` lists the
//! `docs/protocol` files that the C# and Rust suites must both pass. This test
//! loads every listed fixture, checks its recorded SHA-256 and parses each JSON
//! fixture. With `GOLDEN_FIXTURES_RECEIPT` set, it writes the hashes it loaded
//! so CI can prove with `node parity/check-parity.mjs --receipt rust <file>`
//! that this suite did not skip the fixtures.

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rust/ has a parent directory")
        .to_path_buf()
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn manifest(root: &Path) -> Value {
    let path = root.join("parity/golden-fixtures.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_str(&text).expect("parity/golden-fixtures.json is JSON")
}

/// Fixtures that no other Rust test reads get a content check here.
fn check_specific(path: &str, bytes: &[u8]) {
    if path.ends_with("graphql/deep-fresh.json.gz") {
        assert_eq!(&bytes[..2], &[0x1f, 0x8b], "deep-fresh.json.gz is gzip");
    }
    if path.ends_with("graphql/deepclient-operations.json") {
        let operations: Value = serde_json::from_slice(bytes).expect("deepclient-operations.json");
        let operations = operations
            .as_array()
            .expect("a list of recorded operations");
        // Entries are recorded operations, plus one transport descriptor
        // (`protocol`, `websocket`) that has no query.
        let recorded: Vec<&Value> = operations
            .iter()
            .filter(|operation| operation.get("query").is_some())
            .collect();
        assert!(
            !recorded.is_empty(),
            "deepclient-operations.json has no operations"
        );
        for operation in recorded {
            assert!(operation["operationName"].is_string(), "{operation}");
            assert!(operation["query"].is_string(), "{operation}");
        }
    }
}

#[test]
fn rust_suite_loads_every_golden_fixture() {
    let root = repository_root();
    let manifest = manifest(&root);
    let fixtures = manifest["fixtures"].as_array().expect("fixtures array");
    assert!(!fixtures.is_empty(), "the golden fixture list is empty");
    let mut loaded = Map::new();
    for fixture in fixtures {
        let path = fixture["path"].as_str().expect("fixture path");
        let bytes = fs::read(root.join(path)).unwrap_or_else(|error| panic!("{path}: {error}"));
        let actual = sha256_hex(&bytes);
        assert_eq!(
            fixture["sha256"].as_str(),
            Some(actual.as_str()),
            "{path} changed; review both suites, then run node parity/check-parity.mjs --update"
        );
        if path.ends_with(".json") {
            serde_json::from_slice::<Value>(&bytes)
                .unwrap_or_else(|error| panic!("{path} is not JSON: {error}"));
        }
        check_specific(path, &bytes);
        loaded.insert(path.to_owned(), Value::String(actual));
    }
    if let Ok(receipt) = std::env::var("GOLDEN_FIXTURES_RECEIPT") {
        let body = json!({ "suite": "rust", "fixtures": loaded });
        fs::write(&receipt, format!("{body:#}\n"))
            .unwrap_or_else(|error| panic!("cannot write {receipt}: {error}"));
    }
}

#[test]
fn hashes_are_lowercase_hex() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
