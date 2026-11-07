//! One portable format consumed by native Rust, actual WASM and C#.
use link_cli::transactions::JournalRecord;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn shared_complete_record_vectors_preserve_names_and_row_compatibility() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/transaction-name-vectors.json"
    ))
    .unwrap();
    for row in vectors["records"].as_array().unwrap() {
        let wire = row["wire"].as_str().unwrap();
        let actual = JournalRecord::<u32>::parse(wire).unwrap();
        assert_eq!(actual.serialize(), wire);
        assert_eq!(actual.name_only, row["nameOnly"].as_bool().unwrap());
        let expected = row["names"].as_array().unwrap();
        assert_eq!(actual.names.len(), expected.len());
        for (name, expected) in actual.names.iter().zip(expected) {
            assert_eq!(name.index.to_string(), expected["index"].as_str().unwrap());
            assert_eq!(name.before.as_deref(), expected["before"].as_str());
            assert_eq!(name.after.as_deref(), expected["after"].as_str());
        }
    }
    for field in ["invalid", "nonUnicode"] {
        for row in vectors[field].as_array().unwrap() {
            assert!(
                JournalRecord::<u32>::parse(row.as_str().unwrap()).is_err(),
                "{row}"
            );
        }
    }
    for row in vectors["wide"].as_array().unwrap() {
        let text = row.as_str().unwrap();
        assert!(matches!(
            JournalRecord::<u32>::parse(text),
            Err(link_cli::LinkError::AddressOutOfRange(_))
        ));
        assert_eq!(JournalRecord::<u64>::parse(text).unwrap().serialize(), text);
    }
    for row in vectors["legacy"].as_array().unwrap() {
        let record = JournalRecord::<u32>::parse(row.as_str().unwrap()).unwrap();
        assert_eq!(record.serialize(), record.transition.serialize());
        assert!(!record.name_only);
        assert!(record.names.is_empty());
    }
}
