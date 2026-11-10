//! Tests for the version-control decorator, extracted from `mod.rs` for
//! issue #172 to keep that file below the 900-line warning threshold.

use super::*;

fn vectors() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/protocol/version-metadata-vectors.json"
    ))
    .unwrap()
}

#[test]
fn shared_branch_vectors_preserve_names_and_legacy_records() {
    for value in vectors()["branches"].as_array().unwrap() {
        let expected = BranchInfo::new(
            value["name"].as_str().unwrap().into(),
            value["parent"].as_str().map(str::to_string),
            value["fork"].as_i64().unwrap(),
            value["head"].as_i64().unwrap(),
        );
        let marker = value["marker"].as_str().unwrap();
        assert_eq!(
            try_decode_branch_marker(marker),
            Some(expected.clone()),
            "{marker}"
        );
        if value["version"] == 2 {
            assert_eq!(encode_branch_marker(&expected), marker);
        }
    }
}

#[test]
fn shared_malformed_branch_vectors_are_rejected() {
    for marker in vectors()["invalidBranches"].as_array().unwrap() {
        assert!(
            try_decode_branch_marker(marker.as_str().unwrap()).is_none(),
            "{marker}"
        );
    }
}

#[test]
fn version_name_decoder_accepts_lowercase_and_rejects_invalid_utf16() {
    assert_eq!(decode_version_name("006a006b"), Some("jk".into()));
    for invalid in [
        "D800",
        "DFFF",
        "D8000041",
        "éé",
        "\u{ff10}\u{ff10}\u{ff10}\u{ff10}",
    ] {
        assert!(decode_version_name(invalid).is_none(), "{invalid}");
    }
}

#[test]
fn encode_round_trips_through_decode() {
    let info = BranchInfo::new("feature".into(), Some("main".into()), 5, 9);
    let text = encode_branch_marker(&info);
    let decoded = try_decode_branch_marker(&text).unwrap();
    assert_eq!(info, decoded);
}

#[test]
fn make_version_control_database_filename_returns_sibling_path() {
    let path =
        VersionControlDecorator::make_version_control_database_filename("/var/data/db.links");
    assert_eq!(path, PathBuf::from("/var/data/db.versioncontrol.links"));
}

#[test]
fn decode_branch_marker_rejects_invalid_input() {
    assert!(try_decode_branch_marker("not a marker").is_none());
    assert!(try_decode_branch_marker("__vc:branch:x:parent=:fork=z:head=1").is_none());
}
