use super::*;

fn open(
    catalog: &Store,
    a: &Store,
    b: &Store,
    budget: usize,
) -> PartitionedJournalStore<Store, Store> {
    PartitionedJournalStore::new(
        catalog.clone(),
        BTreeMap::from([("east".into(), a.clone()), ("west".into(), b.clone())]),
        |key| {
            if key == "a" {
                "east".into()
            } else {
                "west".into()
            }
        },
    )
    .unwrap()
    .with_object_cache(budget)
}
fn put(writer: &mut PartitionedJournalStore<Store, Store>, key: &str, entries: &[&str]) {
    let keys = [key.into()];
    let expected = writer.read(&keys).unwrap();
    assert!(writer
        .compare_exchange(
            &keys,
            &expected,
            &[Some(entries.iter().map(|s| (*s).into()).collect())]
        )
        .unwrap());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_independent_lru_oracle_matches_payload_costs() {
    let rows: Vec<Vec<&str>> = vec![
        vec![],
        vec![""],
        vec!["é"],
        vec!["abc"],
        vec!["雪"],
        vec!["😀"],
    ];
    let costs = [1, 2, 4, 5, 5, 6];
    let ids = ["a", "b", "c", "d", "e", "f"];
    for budget in [0, 1, 2, 4, 9, 17] {
        let (catalog, a, b, mut writer) = fixture();
        for (id, row) in ids.iter().zip(&rows) {
            put(&mut writer, id, row);
        }
        let mut reader = open(&catalog, &a, &b, budget);
        let start = a.0.lock().unwrap().reads + b.0.lock().unwrap().reads;
        let mut order = Vec::<usize>::new();
        let mut used = 0;
        let mut misses = 0;
        let mut seed = 138u32;
        for _ in 0..128 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let selected = ((seed >> 16) % 6) as usize;
            if let Some(position) = order.iter().position(|id| *id == selected) {
                order.remove(position);
                order.push(selected);
            } else {
                misses += 1;
                if costs[selected] <= budget {
                    while used + costs[selected] > budget {
                        used -= costs[order.remove(0)];
                    }
                    used += costs[selected];
                    order.push(selected);
                }
            }
            assert_eq!(
                reader.read(&[ids[selected].into()]).unwrap()[0].entries,
                rows[selected]
            );
            assert_eq!(
                a.0.lock().unwrap().reads + b.0.lock().unwrap().reads,
                start + misses
            );
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn empty_objects_have_positive_weight_and_partitions_never_alias() {
    for (budget, downloads) in [(1, 3), (2, 2)] {
        let (catalog, a, b, mut writer) = fixture();
        put(&mut writer, "a", &[]);
        put(&mut writer, "b", &[]);
        put(&mut writer, "c", &["é"]);
        let mut reader = open(&catalog, &a, &b, budget);
        let reads = || a.0.lock().unwrap().reads + b.0.lock().unwrap().reads;
        let start = reads();
        for key in ["a", "b", "a"] {
            reader.read(&[key.into()]).unwrap();
        }
        assert_eq!(reads(), start + downloads);
        reader.read(&["c".into()]).unwrap();
        reader.read(&["a".into()]).unwrap();
        assert_eq!(reads(), start + downloads + 1);
        catalog
            .0
            .lock()
            .unwrap()
            .values
            .get_mut("a")
            .unwrap()
            .generation = 99;
        assert_eq!(reader.read(&["a".into()]).unwrap()[0].generation, 99);
        catalog.0.lock().unwrap().values.remove("a");
        assert_eq!(
            reader.read(&["a".into()]).unwrap(),
            [JournalVersion::default()]
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn failed_fetches_can_be_repaired_and_staging_always_compares_full_contents() {
    let (catalog, a, b, mut writer) = fixture();
    put(&mut writer, "a", &["é"]);
    let (address, object) =
        a.0.lock()
            .unwrap()
            .values
            .first_key_value()
            .map(|(k, v)| (k.clone(), v.clone()))
            .unwrap();
    let mut reader = open(&catalog, &a, &b, 4);
    let key = ["a".into()];
    let start = a.0.lock().unwrap().reads;
    a.0.lock()
        .unwrap()
        .values
        .get_mut(&address)
        .unwrap()
        .entries = vec!["broken".into()];
    assert!(reader.read(&key).is_err());
    a.0.lock()
        .unwrap()
        .values
        .insert(address.clone(), object.clone());
    let expected = reader.read(&key).unwrap();
    reader.read(&key).unwrap();
    assert_eq!(a.0.lock().unwrap().reads, start + 2);
    a.0.lock()
        .unwrap()
        .values
        .get_mut(&address)
        .unwrap()
        .entries = vec!["broken".into()];
    assert!(reader
        .compare_exchange(&key, &expected, &[Some(vec!["é".into()])])
        .is_err());
    assert_eq!(
        catalog.0.lock().unwrap().values["a"].generation,
        expected[0].generation
    );
    a.0.lock().unwrap().values.insert(address, object);
    assert!(reader
        .compare_exchange(&key, &expected, &[Some(vec!["é".into()])])
        .unwrap());
    assert_ne!(
        reader.read(&key).unwrap()[0].generation,
        expected[0].generation
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn rejected_and_uncertain_publications_resolve_from_fresh_catalog_state() {
    let (catalog, a, b, mut writer) = fixture();
    put(&mut writer, "a", &["old"]);
    let mut reader = open(&catalog, &a, &b, 20);
    let key = ["a".into()];
    let old = reader.read(&key).unwrap();
    put(&mut writer, "a", &["new"]);
    assert!(!reader
        .compare_exchange(&key, &old, &[Some(vec!["orphan".into()])])
        .unwrap());
    let current = reader.read(&key).unwrap();
    assert_eq!(current[0].entries, ["new"]);
    let update = [Some(vec!["next".into()])];
    catalog.0.lock().unwrap().fail = true;
    assert!(reader.compare_exchange(&key, &current, &update).is_err());
    catalog.0.lock().unwrap().fail = false;
    assert_eq!(reader.read(&key).unwrap()[0].entries, ["new"]);
    catalog.0.lock().unwrap().lose_reply = true;
    assert!(reader.compare_exchange(&key, &current, &update).is_err());
    catalog.0.lock().unwrap().lose_reply = false;
    assert_eq!(reader.read(&key).unwrap()[0].entries, ["next"]);
    let mut disabled = reader.with_object_cache(0);
    let start = a.0.lock().unwrap().reads;
    disabled.read(&key).unwrap();
    disabled.read(&key).unwrap();
    assert_eq!(a.0.lock().unwrap().reads, start + 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn immutable_cache_keeps_catalog_fresh_and_returns_owned_entries() {
    let (catalog, a, b, mut writer) = fixture();
    let key = vec!["a".into()];
    let base = writer.read(&key).unwrap();
    writer
        .compare_exchange(&key, &base, &[Some(vec!["é".into()])])
        .unwrap();
    let mut reader = PartitionedJournalStore::new(
        catalog.clone(),
        BTreeMap::from([("east".into(), a.clone()), ("west".into(), b)]),
        |_| "east".into(),
    )
    .unwrap()
    .with_object_cache(4);
    let before = a.0.lock().unwrap().reads;
    let mut first = reader.read(&key).unwrap();
    assert_eq!(a.0.lock().unwrap().reads, before + 1);
    first[0].entries[0] = "changed caller buffer".into();
    a.0.lock().unwrap().fail = true;
    assert_eq!(reader.read(&key).unwrap()[0].entries, ["é"]);
    assert_eq!(a.0.lock().unwrap().reads, before + 1);
    a.0.lock().unwrap().fail = false;
    let base = writer.read(&key).unwrap();
    writer
        .compare_exchange(&key, &base, &[Some(vec!["x".into()])])
        .unwrap();
    let next = reader.read(&key).unwrap();
    assert!(next[0].generation > first[0].generation);
    assert_eq!(next[0].entries, ["x"]);
    assert!(!reader.compare_exchange(&key, &first, &[None]).unwrap());
    catalog.0.lock().unwrap().fail = true;
    assert!(reader.read(&key).is_err());
    catalog.0.lock().unwrap().fail = false;
    assert_eq!(reader.read(&key).unwrap()[0].entries, ["x"]);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn immutable_cache_uses_exact_utf8_budget_and_lru_promotion() {
    let (catalog, a, b, mut writer) = fixture();
    let selected = vec!["a".into(), "b".into(), "c".into()];
    let base = writer.read(&selected).unwrap();
    writer
        .compare_exchange(
            &selected,
            &base,
            &[
                Some(vec!["é".into()]),
                Some(vec!["abc".into()]),
                Some(vec![String::new()]),
            ],
        )
        .unwrap();
    let mut reader = PartitionedJournalStore::new(
        catalog,
        BTreeMap::from([("east".into(), a.clone()), ("west".into(), b.clone())]),
        |key| {
            if key == "a" {
                "east".into()
            } else {
                "west".into()
            }
        },
    )
    .unwrap()
    .with_object_cache(9);
    let reads = || a.0.lock().unwrap().reads + b.0.lock().unwrap().reads;
    let start = reads();
    for id in ["a", "b", "a", "c"] {
        reader.read(&[id.into()]).unwrap();
    }
    assert_eq!(reads(), start + 3); // a=4, b=5, c=2; b is least recently used.
    reader.read(&["b".into()]).unwrap();
    assert_eq!(reads(), start + 4);
    reader.read(&["c".into()]).unwrap();
    assert_eq!(reads(), start + 4);
    reader.read(&["a".into()]).unwrap();
    assert_eq!(reads(), start + 5);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn immutable_cache_zero_oversize_and_generation_mismatches_never_hit() {
    for budget in [0, 3, 4] {
        let (catalog, a, b, mut writer) = fixture();
        let key = vec!["a".into()];
        let base = writer.read(&key).unwrap();
        writer
            .compare_exchange(&key, &base, &[Some(vec!["é".into()])])
            .unwrap();
        let mut reader = PartitionedJournalStore::new(
            catalog.clone(),
            BTreeMap::from([("east".into(), a.clone()), ("west".into(), b)]),
            |_| "east".into(),
        )
        .unwrap()
        .with_object_cache(budget);
        let start = a.0.lock().unwrap().reads;
        reader.read(&key).unwrap();
        reader.read(&key).unwrap();
        assert_eq!(
            a.0.lock().unwrap().reads - start,
            if budget == 4 { 1 } else { 2 }
        );
        catalog
            .0
            .lock()
            .unwrap()
            .values
            .get_mut("a")
            .unwrap()
            .entries[3] = "99".into();
        assert!(reader.read(&key).is_err());
        assert!(reader.read(&key).is_err());
        assert_eq!(
            a.0.lock().unwrap().reads - start,
            if budget == 4 { 3 } else { 4 }
        );
    }
}
