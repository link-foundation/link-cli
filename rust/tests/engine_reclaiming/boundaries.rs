use super::*;
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unsupported_chunk_manifests_never_reach_deletion() {
    for entries in [
        vec!["chunks1", "east", "0", "0"],
        vec![
            "chunks1",
            "east",
            "1",
            "1",
            "blob1-0000000000000000",
            "1",
            "1",
        ],
    ] {
        let (c, a, b, mut store) = fixture();
        write(&mut store, "a0", "b0");
        write(&mut store, "a1", "b1");
        let before_a = a.0.lock().unwrap().values.clone();
        let before_b = b.0.lock().unwrap().values.clone();
        c.0.lock().unwrap().values.get_mut("a").unwrap().entries =
            entries.into_iter().map(String::from).collect();
        assert!(store.collect_garbage().is_err());
        assert_eq!(a.count("delete") + b.count("delete"), 0);
        assert_eq!(a.0.lock().unwrap().values, before_a);
        assert_eq!(b.0.lock().unwrap().values, before_b);
    }
}
fn version(generation: u64, records: &[&str]) -> JournalVersion {
    JournalVersion {
        generation,
        entries: records.iter().map(|s| (*s).into()).collect(),
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn malformed_complete_snapshots_delete_nothing_in_any_partition() {
    for phase in 0..3 {
        for damage in [
            "empty",
            "zero",
            "unordered",
            "duplicate",
            "name",
            "format",
            "entries",
        ] {
            let (c, a, b, mut store) = fixture();
            write(&mut store, "a0", "b0");
            write(&mut store, "a1", "b1");
            let provider = [&c, &a, &b][phase];
            let entry = |key: &str, value: JournalVersion| JournalNamespaceEntry {
                key: key.into(),
                version: value,
            };
            let items = match damage {
                "empty" => vec![],
                "zero" => vec![entry("a", version(0, &[]))],
                "unordered" => vec![entry("z", version(1, &[])), entry("a", version(2, &[]))],
                "duplicate" => vec![entry("a", version(1, &[])), entry("a", version(1, &[]))],
                "name" => vec![entry("../bad", version(1, &[]))],
                "format" => vec![entry("unrecognized", version(1, &[]))],
                _ => vec![entry(
                    if phase == 0 { EPOCH } else { "a" },
                    version(1, &["extra"]),
                )],
            };
            provider.0.lock().unwrap().scan_override = Some(items);
            assert!(
                store.collect_garbage().is_err(),
                "phase={phase} damage={damage}"
            );
            assert_eq!(a.count("delete") + b.count("delete"), 0);
        }
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn contradictory_roots_and_changed_protected_generations_are_rejected() {
    for damage in [
        "contradictory",
        "generation",
        "absent",
        "future",
        "old-snapshot",
    ] {
        let (c, a, b, mut store) = fixture();
        write(&mut store, "a0", "b0");
        write(&mut store, "a1", "b1");
        let pointer = c.0.lock().unwrap().values["a"].clone();
        match damage {
            "contradictory" => {
                let mut bad = pointer.clone();
                bad.entries[3] = "999".into();
                c.0.lock().unwrap().values.insert("z".into(), bad);
            }
            "generation" => {
                a.0.lock()
                    .unwrap()
                    .values
                    .get_mut(&pointer.entries[2])
                    .unwrap()
                    .generation = 999
            }
            "absent" => {
                a.0.lock().unwrap().values.remove(&pointer.entries[2]);
            }
            "future" => {
                let mut bad = pointer;
                bad.entries[2].replace_range(6..22, "ffffffffffffffff");
                c.0.lock().unwrap().values.insert("a".into(), bad);
            }
            _ => {
                let prior = c.clone().scan_namespace(Default::default()).unwrap();
                c.0.lock().unwrap().scan_override = Some(prior);
            }
        }
        assert!(store.collect_garbage().is_err(), "{damage}");
        assert_eq!(a.count("delete") + b.count("delete"), 0);
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn provider_cannot_bypass_selected_scan_limits() {
    for options in [
        ReclamationOptions {
            max_keys: 1,
            ..Default::default()
        },
        ReclamationOptions {
            max_bytes: 32,
            ..Default::default()
        },
    ] {
        let c = Store::default();
        let a = Store::default();
        let b = Store::default();
        let mut store = open(&c, &a, &b, options);
        write(&mut store, "a0", "b0");
        let complete = c.clone().scan_namespace(Default::default()).unwrap();
        c.0.lock().unwrap().scan_override = Some(complete);
        assert!(store.collect_garbage().is_err());
        assert_eq!(a.count("delete") + b.count("delete"), 0);
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn ownership_checks_and_staging_collision_preserve_catalog() {
    let c = Store::default();
    let a = Store::default();
    a.0.lock()
        .unwrap()
        .values
        .insert("foreign".into(), version(1, &[]));
    assert!(Reclaiming::open(
        c.clone(),
        BTreeMap::from([("east".into(), a.clone())]),
        |_| "east".into(),
        Default::default()
    )
    .is_err());
    assert_eq!(c.count("swap"), 0);
    a.0.lock().unwrap().values.clear();
    c.0.lock()
        .unwrap()
        .values
        .insert("a".into(), version(1, &["blob1", "east", "legacy", "1"]));
    assert!(Reclaiming::open(
        c.clone(),
        BTreeMap::from([("east".into(), a.clone())]),
        |_| "east".into(),
        Default::default()
    )
    .is_err());
    assert_eq!(c.count("swap"), 0);
    let (c, a, _, mut store) = fixture();
    let expected = write(&mut store, "live", "b");
    let count = a.count("swap");
    assert!(store
        .compare_exchange(&keys(), &expected, &[Some(vec!["live".into()]), None])
        .unwrap());
    assert_eq!(a.count("swap"), count);
    a.0.lock()
        .unwrap()
        .values
        .values_mut()
        .next()
        .unwrap()
        .entries = vec!["collision".into()];
    let expected = store.read(&["b".into()]).unwrap();
    assert!(!expected.is_empty());
    // Catalog generations remain fresh without fetching corrupt immutable data.
    let current = c.0.lock().unwrap().values["a"].clone();
    let count = c.count("swap");
    assert!(store
        .compare_exchange(&["a".into()], &[current], &[Some(vec!["live".into()])])
        .is_err());
    assert_eq!(c.count("swap"), count);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn changed_catalog_pointer_cannot_reuse_generation_during_refresh() {
    let (c, a, b, mut writer) = fixture();
    let mut reader = open(&c, &a, &b, Default::default());
    write(&mut writer, "old", "b");
    let generation = c.0.lock().unwrap().values["a"].generation;
    let catalog = c.clone();
    a.on("read", false, move || {
        write(&mut writer, "new", "b");
        catalog
            .0
            .lock()
            .unwrap()
            .values
            .get_mut("a")
            .unwrap()
            .generation = generation;
        writer.collect_garbage()?;
        Ok(())
    });
    assert!(reader.read(&["a".into()]).is_err());
}
