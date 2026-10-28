use super::*;
fn publish(store: &mut Reclaiming, value: &str) {
    let key = vec!["a".into()];
    let before = store.read(&key).unwrap();
    assert!(store
        .compare_exchange(&key, &before, &[Some(vec![value.into()])])
        .unwrap());
    assert_eq!(store.read(&key).unwrap()[0].entries, [value]);
}
fn mutations(a: &Store, b: &Store) -> usize {
    a.count("delete") + b.count("delete")
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unknown_initialization_stage_publication_and_fence_are_never_replayed() {
    for after in [false, true] {
        let c = Store::default();
        let a = Store::default();
        let b = Store::default();
        c.on("swap", after, || Err(error("initialization response lost")));
        assert!(Reclaiming::open(
            c.clone(),
            BTreeMap::from([("east".into(), a.clone())]),
            |_| "east".into(),
            Default::default()
        )
        .is_err());
        assert_eq!(c.count("swap"), 1);
        assert_eq!(c.0.lock().unwrap().values.contains_key(EPOCH), after);
        assert_eq!(mutations(&a, &b), 0);
        let _ = open(&c, &a, &b, Default::default());
        for operation in ["stage", "publish", "fence"] {
            let (c, a, b, mut store) = fixture();
            publish(&mut store, "old");
            let provider = if operation == "stage" { &a } else { &c };
            let before = provider.count("swap");
            provider.on("swap", after, || Err(error("response lost")));
            if operation == "fence" {
                assert!(store.collect_garbage().is_err());
            } else {
                let key = vec!["a".into()];
                let current = store.read(&key).unwrap();
                assert!(store
                    .compare_exchange(&key, &current, &[Some(vec!["new".into()])])
                    .is_err());
            }
            assert_eq!(provider.count("swap"), before + 1);
            assert_eq!(mutations(&a, &b), 0);
            let expected = if operation == "publish" && after {
                "new"
            } else {
                "old"
            };
            assert_eq!(store.read(&["a".into()]).unwrap()[0].entries, [expected]);
            publish(&mut store, "following");
        }
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn known_competitors_and_exhaustion_preserve_mutation_outcomes() {
    let (c, a, b, mut store) = fixture();
    let mut other = open(&c, &a, &b, Default::default());
    c.on("swap", false, move || {
        assert_eq!(other.collect_garbage()?.unwrap().epoch, 1);
        Ok(())
    });
    assert_eq!(store.collect_garbage().unwrap(), None);
    assert_eq!(mutations(&a, &b), 0);
    c.0.lock().unwrap().values.get_mut(EPOCH).unwrap().entries[1] = u64::MAX.to_string();
    let before = c.count("swap");
    assert!(store.collect_garbage().is_err());
    assert_eq!(c.count("swap"), before);
    let c = Store::default();
    let a = Store::default();
    let b = Store::default();
    let competing = c.clone();
    c.on("swap", false, move || {
        let mut state = competing.0.lock().unwrap();
        state.revision = 1;
        state.values.insert(
            EPOCH.into(),
            JournalVersion {
                generation: 1,
                entries: vec!["gc1".into(), "7".into()],
            },
        );
        Ok(())
    });
    let mut store = open(&c, &a, &b, Default::default());
    assert_eq!(store.collect_garbage().unwrap().unwrap().epoch, 8);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn generation_delete_guard_skips_aba_replacement() {
    let (_, a, _, mut store) = fixture();
    publish(&mut store, "old");
    publish(&mut store, "new");
    let retired =
        a.0.lock()
            .unwrap()
            .values
            .iter()
            .find(|(_, v)| v.entries == ["old"])
            .unwrap()
            .0
            .clone();
    let replacement = a.clone();
    let key = retired.clone();
    a.on("delete", false, move || {
        let mut state = replacement.0.lock().unwrap();
        state.revision += 1;
        let generation = state.revision;
        state.values.insert(
            key,
            JournalVersion {
                generation,
                entries: vec!["ABA replacement".into()],
            },
        );
        Ok(())
    });
    assert_eq!(store.collect_garbage().unwrap().unwrap().deleted, 0);
    assert_eq!(
        a.0.lock().unwrap().values[&retired].entries,
        ["ABA replacement"]
    );
    assert_eq!(store.read(&["a".into()]).unwrap()[0].entries, ["new"]);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn reader_damage_fails_closed_and_only_absence_refreshes() {
    for damage in ["absent", "generation", "content", "transport", "short"] {
        let (c, a, b, mut store) = fixture();
        publish(&mut store, "live");
        {
            let mut state = a.0.lock().unwrap();
            match damage {
                "absent" => state.values.clear(),
                "generation" => state.values.values_mut().next().unwrap().generation = 999,
                "content" => {
                    state.values.values_mut().next().unwrap().entries = vec!["changed".into()]
                }
                "short" => state.read_override = Some(vec![]),
                _ => (),
            }
        }
        if damage == "transport" {
            a.on("read", false, || Err(error("data outage")));
        }
        let before = c.count("read");
        assert!(store.read(&["a".into()]).is_err());
        assert_eq!(
            c.count("read") - before,
            if damage == "absent" { 2 } else { 1 }
        );
        assert_eq!(mutations(&a, &b), 0);
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn cache_preserves_owned_data_after_collection_but_requires_live_catalog() {
    let c = Store::default();
    let a = Store::default();
    let b = Store::default();
    let mut store = open(
        &c,
        &a,
        &b,
        ReclamationOptions {
            cache_bytes: 64,
            ..Default::default()
        },
    );
    publish(&mut store, "old");
    let owned = store.read(&["a".into()]).unwrap();
    publish(&mut store, "new");
    assert_eq!(store.collect_garbage().unwrap().unwrap().deleted, 1);
    a.on("read", false, || Err(error("data outage")));
    assert_eq!(store.read(&["a".into()]).unwrap()[0].entries, ["new"]);
    assert_eq!(owned[0].entries, ["old"]);
    c.on("read", false, || Err(error("catalog outage")));
    assert!(store.read(&["a".into()]).is_err());
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn validation_precedes_io_and_readonly_group_reserves_epoch_slot() {
    let c = Store::default();
    let a = Store::default();
    for options in [
        ReclamationOptions {
            max_keys: 0,
            ..Default::default()
        },
        ReclamationOptions {
            max_bytes: 0,
            ..Default::default()
        },
        ReclamationOptions {
            page_keys: 0,
            ..Default::default()
        },
        ReclamationOptions {
            page_keys: 257,
            ..Default::default()
        },
    ] {
        assert!(Reclaiming::open(
            c.clone(),
            BTreeMap::from([("east".into(), a.clone())]),
            |_| "east".into(),
            options
        )
        .is_err());
    }
    assert_eq!(c.count("scan"), 0);
    let (c, a, b, mut store) = fixture();
    let keys = (0..31).map(|i| format!("s{i}")).collect::<Vec<_>>();
    let before = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &before, &vec![None; 31])
        .unwrap());
    assert_eq!(c.0.lock().unwrap().calls.last().unwrap().1.len(), 32);
    assert!(a.0.lock().unwrap().values.is_empty());
    let old = store.read(&["a".into()]).unwrap();
    publish(&mut store, "new");
    let count = a.count("read");
    assert!(!store
        .compare_exchange(&["a".into()], &old, &[Some(vec!["orphan".into()])])
        .unwrap());
    assert_eq!(a.count("read"), count);
    let before = c.count("read");
    assert!(store.compare_exchange(&["a".into()], &[], &[None]).is_err());
    assert_eq!(c.count("read"), before);
    assert_eq!(mutations(&a, &b), 0);
}
