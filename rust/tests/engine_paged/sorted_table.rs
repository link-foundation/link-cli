use super::*;
use link_cli::engine::paged::{SortedJournalEntry, SortedJournalTable, SortedTableOptions};
fn rows() -> Vec<SortedJournalEntry> {
    vec![
        SortedJournalEntry {
            key: "a".into(),
            generation: 1,
            entries: Some(vec!["\u{feff}雪\0😀\n".into()]),
        },
        SortedJournalEntry {
            key: "b".into(),
            generation: u64::MAX,
            entries: None,
        },
        SortedJournalEntry {
            key: "c".into(),
            generation: 3,
            entries: Some(vec![]),
        },
    ]
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn sealed_sorted_table_roundtrip_tombstones_and_owned_values() {
    let device = Device::default();
    let mut table =
        SortedJournalTable::create(device.clone(), &rows(), Default::default()).unwrap();
    assert_eq!(table.len(), 3);
    assert!(!table.is_empty());
    assert!(table.supports_durable_flush());
    assert_eq!(table.get("a").unwrap(), Some(rows()[0].clone()));
    assert_eq!(table.get("b").unwrap(), Some(rows()[1].clone()));
    assert_eq!(table.get("c").unwrap(), Some(rows()[2].clone()));
    assert_eq!(table.get("absent").unwrap(), None);
    assert_eq!(table.entries().unwrap(), rows());
    let mut owned = table.get("a").unwrap().unwrap();
    owned.entries.as_mut().unwrap()[0] = "caller".into();
    assert_eq!(table.get("a").unwrap(), Some(rows()[0].clone()));
    assert_eq!(
        SortedJournalTable::open(device.crash(), Default::default())
            .unwrap()
            .entries()
            .unwrap(),
        rows()
    );
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn table_rejects_unsorted_duplicate_and_zero_generation_before_io() {
    for input in [
        vec![rows()[1].clone(), rows()[0].clone()],
        vec![rows()[0].clone(); 2],
        vec![SortedJournalEntry {
            generation: 0,
            ..rows()[0].clone()
        }],
    ] {
        let device = Device::default();
        assert!(SortedJournalTable::create(device.clone(), &input, Default::default()).is_err());
        assert!(device.0.borrow().calls.is_empty());
        assert_eq!(device.0.borrow().reads, 0);
        assert_eq!(device.0.borrow().live, Image::default());
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn table_point_lookup_reads_selected_pages_and_detects_post_open_damage() {
    let input: Vec<_> = (0..80)
        .map(|i| SortedJournalEntry {
            key: format!("k{i:03}"),
            generation: i + 1,
            entries: Some(vec!["x".repeat(1000)]),
        })
        .collect();
    let device = Device::default();
    let mut table = SortedJournalTable::create(device.clone(), &input, Default::default()).unwrap();
    assert!(device.0.borrow().live.pages.len() > 40);
    device.0.borrow_mut().calls.clear();
    assert_eq!(table.get("k040").unwrap(), Some(input[40].clone()));
    assert!(device.0.borrow().calls.len() <= 3);
    let end = device.0.borrow().live.pages.len() - 1;
    device.0.borrow_mut().live.pages[end][64] ^= 1;
    assert!(table.get("k079").is_err());
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn table_limits_and_existing_data_are_preserved() {
    let device = Device::default();
    assert!(SortedJournalTable::create(
        device.clone(),
        &rows(),
        SortedTableOptions {
            max_keys: 2,
            ..Default::default()
        }
    )
    .is_err());
    assert!(device.0.borrow().calls.is_empty());
    assert_eq!(device.0.borrow().reads, 0);
    SortedJournalTable::create(device.clone(), &rows(), Default::default()).unwrap();
    let before = device.0.borrow().live.clone();
    assert!(SortedJournalTable::create(device.clone(), &rows(), Default::default()).is_err());
    assert_eq!(device.0.borrow().live, before);
    assert!(SortedJournalTable::open(
        device.clone(),
        SortedTableOptions {
            max_bytes: 8,
            ..Default::default()
        }
    )
    .is_err());
    assert_eq!(device.0.borrow().live, before);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_sorted_table_vectors_are_accepted_or_rejected_without_mutation() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/protocol/native-sorted-table-v1.json"
    ))
    .unwrap();
    for case in vectors["cases"].as_array().unwrap() {
        let image = Image {
            metadata: std::array::from_fn(|i| {
                case["metadata"][i]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(hex)
            }),
            pages: case["pages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| hex(s.as_str().unwrap()).try_into().unwrap())
                .collect(),
        };
        let device = Device::from_image(image.clone());
        let result = SortedJournalTable::open(device.clone(), Default::default());
        assert_eq!(
            result.is_ok(),
            case["valid"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(device.0.borrow().live, image);
        assert_eq!(device.0.borrow().durable, image);
        if let Ok(mut table) = result {
            let expected: Vec<_> = case["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| SortedJournalEntry {
                    key: r[0].as_str().unwrap().into(),
                    generation: r[1].as_str().unwrap().parse().unwrap(),
                    entries: if r[2].is_null() {
                        None
                    } else {
                        Some(
                            r[2].as_array()
                                .unwrap()
                                .iter()
                                .map(|v| v.as_str().unwrap().into())
                                .collect(),
                        )
                    },
                })
                .collect();
            assert_eq!(table.entries().unwrap(), expected);
            assert_eq!(table.is_empty(), expected.is_empty());
            assert_eq!(table.identity(), std::array::from_fn(|i| i as u8));
            assert_ne!(table.digest(), 0);
            assert!(table.stored_bytes() >= 15);
        }
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_create_failures_never_expose_a_partial_sealed_table() {
    let probe = Device::default();
    SortedJournalTable::create(probe.clone(), &rows(), Default::default()).unwrap();
    let calls = probe.0.borrow().calls.clone();
    for (i, name) in calls
        .iter()
        .enumerate()
        .filter(|(_, name)| name.as_str() != "readPage")
    {
        for after in [false, true] {
            let device = Device::default();
            device.0.borrow_mut().fault = Some((i + 1, after));
            assert!(
                SortedJournalTable::create(device.clone(), &rows(), Default::default()).is_err(),
                "{i} {name}"
            );
            let recovered = device.crash();
            let before = recovered.0.borrow().live.clone();
            if let Ok(mut table) = SortedJournalTable::open(recovered.clone(), Default::default()) {
                assert_eq!(table.entries().unwrap(), rows());
            }
            assert_eq!(recovered.0.borrow().live, before);
        }
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn torn_table_seal_never_falls_back_to_initial_empty_decision() {
    let device = Device::default();
    SortedJournalTable::create(device.clone(), &rows(), Default::default()).unwrap();
    let good = device.0.borrow().live.clone();
    for length in 0..128 {
        let mut image = good.clone();
        image.metadata[1].as_mut().unwrap().truncate(length);
        let device = Device::from_image(image.clone());
        assert!(SortedJournalTable::open(device.clone(), Default::default()).is_err());
        assert_eq!(device.0.borrow().live, image);
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn sorted_table_conflicts_and_modes_reject_before_completion() {
    let device = Device::default();
    SortedJournalTable::create(device.clone(), &rows(), Default::default()).unwrap();
    let good = device.0.borrow().live.clone();
    for same_generation in [false, true] {
        let mut image = good.clone();
        if same_generation {
            image.metadata[0] = image.metadata[1].clone();
            image.metadata[0].as_mut().unwrap()[72] ^= 1;
        } else {
            image.metadata[0].as_mut().unwrap()[16] ^= 1;
        }
        let bytes = image.metadata[0].as_mut().unwrap();
        let hash = bytes[..120]
            .iter()
            .fold(14695981039346656037_u64, |sum, b| {
                (sum ^ u64::from(*b)).wrapping_mul(1099511628211)
            });
        bytes[120..].copy_from_slice(&hash.to_le_bytes());
        let bad = Device::from_image(image.clone());
        assert!(SortedJournalTable::open(bad.clone(), Default::default()).is_err());
        assert_eq!(bad.0.borrow().completed, 0);
        assert!(bad.0.borrow().calls.is_empty());
        assert_eq!(bad.0.borrow().live, image);
    }
    let mut identical = good.clone();
    identical.metadata[0] = identical.metadata[1].clone();
    let valid = Device::from_image(identical);
    assert_eq!(
        SortedJournalTable::open(valid.clone(), Default::default())
            .unwrap()
            .entries()
            .unwrap(),
        rows()
    );
    assert_eq!(valid.0.borrow().completed, 1);
    let mode = Device::from_image(good);
    mode.0.borrow_mut().mode = 2;
    assert!(SortedJournalTable::open(mode.clone(), Default::default()).is_err());
    assert_eq!(mode.0.borrow().reads, 0);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn sorted_mirrored_table_repairs_only_after_complete_validation() {
    use link_cli::engine::paged::MirroredPageDevice;
    let first = Device::default();
    let second = Device::default();
    SortedJournalTable::create(
        MirroredPageDevice::new(first.clone(), second.clone()),
        &rows(),
        Default::default(),
    )
    .unwrap();
    first.0.borrow_mut().live.pages[0][64] ^= 1;
    first.0.borrow_mut().live.metadata[1]
        .as_mut()
        .unwrap()
        .truncate(17);
    let mut table = SortedJournalTable::open(
        MirroredPageDevice::new(first.clone(), second.clone()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(table.entries().unwrap(), rows());
    assert_eq!(first.0.borrow().live, second.0.borrow().live);
    first.0.borrow_mut().live.pages[0][64] ^= 1;
    second.0.borrow_mut().live.pages[0][64] ^= 2;
    let a = first.0.borrow().live.clone();
    let b = second.0.borrow().live.clone();
    assert!(SortedJournalTable::open(
        MirroredPageDevice::new(first.clone(), second.clone()),
        Default::default()
    )
    .is_err());
    assert_eq!(first.0.borrow().live, a);
    assert_eq!(second.0.borrow().live, b);
}
