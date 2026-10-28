use super::*;
struct Vectors {
    objects: Vec<Object>,
    epochs: Vec<Case>,
    pointers: Vec<Case>,
}
struct Object {
    epoch: String,
    entries: Vec<String>,
    key: String,
}
struct Case {
    entries: Vec<String>,
    valid: bool,
    object_entries: Option<Vec<String>>,
}
fn vectors() -> Vectors {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/protocol/reclamation-v1.json")).unwrap();
    let records = |value: &serde_json::Value| {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap().to_owned())
            .collect()
    };
    let cases = |name: &str| {
        value[name]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| Case {
                entries: records(&item["entries"]),
                valid: item["valid"].as_bool().unwrap(),
                object_entries: item.get("objectEntries").map(records),
            })
            .collect()
    };
    Vectors {
        objects: value["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| Object {
                epoch: item["epoch"].as_str().unwrap().into(),
                key: item["key"].as_str().unwrap().into(),
                entries: records(&item["entries"]),
            })
            .collect(),
        epochs: cases("epochs"),
        pointers: cases("pointers"),
    }
}
fn catalog(epoch: &str) -> Store {
    let store = Store::default();
    {
        let mut state = store.0.lock().unwrap();
        state.revision = 1;
        state.values.insert(
            EPOCH.into(),
            JournalVersion {
                generation: 1,
                entries: vec!["gc1".into(), epoch.into()],
            },
        );
    }
    store
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_addresses_preserve_utf8_framing_and_all_epoch_bits() {
    let vectors = vectors();
    assert_eq!(vectors.objects.len(), 30);
    for case in vectors.objects {
        let c = catalog(&case.epoch);
        let a = Store::default();
        let b = Store::default();
        let mut store = open(&c, &a, &b, Default::default());
        let keys = vec!["a".into()];
        let before = store.read(&keys).unwrap();
        assert!(store
            .compare_exchange(&keys, &before, &[Some(case.entries.clone())])
            .unwrap());
        assert_eq!(
            a.0.lock()
                .unwrap()
                .values
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            [case.key]
        );
        assert_eq!(store.read(&keys).unwrap()[0].entries, case.entries);
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_epochs_validate_before_any_mutation() {
    let vectors = vectors();
    assert_eq!(vectors.epochs.len(), 13);
    for case in vectors.epochs {
        let c = catalog("0");
        c.0.lock().unwrap().values.get_mut(EPOCH).unwrap().entries = case.entries;
        let result = Reclaiming::open(
            c.clone(),
            BTreeMap::from([("east".into(), Store::default())]),
            |_| "east".into(),
            Default::default(),
        );
        assert_eq!(result.is_ok(), case.valid);
        assert_eq!(c.count("swap"), 0);
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_pointers_validate_before_any_mutation() {
    let vectors = vectors();
    assert_eq!(vectors.pointers.len(), 22);
    for case in vectors.pointers {
        let c = catalog("18446744073709551615");
        let a = Store::default();
        let b = Store::default();
        c.0.lock().unwrap().values.insert(
            "a".into(),
            JournalVersion {
                generation: 1,
                entries: case.entries.clone(),
            },
        );
        if case.valid {
            let entries = case.object_entries.unwrap();
            let target = if case.entries[1] == "east" { &a } else { &b };
            target.0.lock().unwrap().values.insert(
                case.entries[2].clone(),
                JournalVersion {
                    generation: case.entries[3].parse().unwrap(),
                    entries: entries.clone(),
                },
            );
            let mut store = open(&c, &a, &b, Default::default());
            assert_eq!(store.read(&["a".into()]).unwrap()[0].entries, entries);
        } else {
            assert!(Reclaiming::open(
                c.clone(),
                BTreeMap::from([("east".into(), a), ("west".into(), b)]),
                |_| "east".into(),
                Default::default()
            )
            .is_err());
        }
        assert_eq!(c.count("swap"), 0);
    }
}
