use super::chunks::{open_chunks, selected};
use super::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_chunk_vectors_preserve_utf8_and_all_epoch_bits() {
    let vectors: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../docs/protocol/reclaiming-chunks-vectors.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 18);
    let strings = |v: &serde_json::Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().into())
            .collect()
    };
    for v in vectors {
        let c = Store::default();
        let a = Store::default();
        let b = Store::default();
        {
            let mut state = c.0.lock().unwrap();
            state.revision = 1;
            state.values.insert(
                EPOCH.into(),
                JournalVersion {
                    generation: 1,
                    entries: vec!["gc1".into(), v["epoch"].as_str().unwrap().into()],
                },
            );
        }
        let mut store = Reclaiming::open_chunked(
            c.clone(),
            BTreeMap::from([("east".into(), a.clone())]),
            |_| "east".into(),
            Default::default(),
            link_cli::engine::partitioned::JournalChunkOptions {
                max_records: v["maxRecords"].as_u64().unwrap() as usize,
                max_bytes: v["maxBytes"].as_u64().unwrap() as usize,
                ..selected()
            },
        )
        .unwrap();
        let keys = vec!["a".into()];
        let data = strings(&v["entries"]);
        let base = store.read(&keys).unwrap();
        assert!(store
            .compare_exchange(&keys, &base, &[Some(data.clone())])
            .unwrap());
        let chunks = v["chunks"].as_array().unwrap();
        let mut expected = vec![
            "chunks2".into(),
            "east".into(),
            data.len().to_string(),
            chunks.len().to_string(),
        ];
        for (index, chunk) in chunks.iter().enumerate() {
            expected.extend([
                chunk["address"].as_str().unwrap().into(),
                (index + 1).to_string(),
                strings(&chunk["entries"]).len().to_string(),
            ]);
        }
        assert_eq!(c.0.lock().unwrap().values["a"].entries, expected);
        assert_eq!(store.read(&keys).unwrap()[0].entries, data);
        let mut reader = open(&c, &a, &b, Default::default());
        assert_eq!(reader.read(&keys).unwrap()[0].entries, data);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn malformed_or_future_chunk_metadata_precedes_all_data_access_and_deletion() {
    let c = Store::default();
    let a = Store::default();
    let b = Store::default();
    let mut store = open_chunks(&c, &a, &b, 0);
    let selected = vec!["a".into()];
    let base = store.read(&selected).unwrap();
    assert!(store
        .compare_exchange(&selected, &base, &[Some(vec!["one".into()])])
        .unwrap());
    let valid = c.0.lock().unwrap().values["a"].entries.clone();
    let k = &valid[4];
    let g = &valid[5];
    let cases: Vec<Vec<&str>> = vec![
        vec!["chunks2"],
        vec!["chunks2", "missing", "0", "0"],
        vec!["chunks2", "east", "00", "0"],
        vec!["chunks2", "east", "4294967296", "0"],
        vec!["chunks2", "east", "0", "4097"],
        vec!["chunks2", "east", "0", "01"],
        vec!["chunks2", "east", "0", "1"],
        vec!["chunks2", "east", "1", "0"],
        vec!["chunks2", "east", "1", "1", "../bad", g, "1"],
        vec!["chunks2", "east", "1", "1", k, "0", "1"],
        vec!["chunks2", "east", "1", "1", k, "01", "1"],
        vec!["chunks2", "east", "1", "1", k, "18446744073709551616", "1"],
        vec!["chunks2", "east", "1", "1", k, g, "0"],
        vec!["chunks2", "east", "1", "1", k, g, "65537"],
        vec!["chunks2", "east", "1", "1", k, g, "-1"],
        vec!["chunks2", "east", "2", "1", k, g, "1"],
        vec![
            "chunks2",
            "east",
            "1",
            "1",
            "blob2-ffffffffffffffff-0000000000000000",
            g,
            "1",
        ],
    ];
    for entries in cases {
        c.0.lock().unwrap().values.insert(
            "b".into(),
            JournalVersion {
                generation: 1,
                entries: entries.into_iter().map(String::from).collect(),
            },
        );
        let reads = a.count("read");
        assert!(store.read(&keys()).is_err());
        assert_eq!(a.count("read"), reads);
        assert!(store.collect_garbage().is_err());
        assert_eq!(a.count("delete") + b.count("delete"), 0);
        c.0.lock().unwrap().values.get_mut(EPOCH).unwrap().entries[1] = "0".into();
    }
}
