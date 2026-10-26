use super::*;
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_partition_configuration_performs_no_io() {
    for name in [
        None,
        Some("".into()),
        Some("bad/name".into()),
        Some("a".repeat(129)),
    ] {
        let c = Store::default();
        let a = Store::default();
        let partitions = name.into_iter().map(|name| (name, a.clone())).collect();
        assert!(
            Reclaiming::open(c.clone(), partitions, |_| "east".into(), Default::default()).is_err()
        );
        assert!(c.0.lock().unwrap().calls.is_empty());
        assert!(a.0.lock().unwrap().calls.is_empty());
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn all_routes_validate_before_io_and_readonly_dependencies_need_no_route() {
    let c = Store::default();
    let a = Store::default();
    let mut store = Reclaiming::open(
        c.clone(),
        BTreeMap::from([("east".into(), a.clone())]),
        |key| {
            if key == "a" {
                "east".into()
            } else {
                "unconfigured".into()
            }
        },
        Default::default(),
    )
    .unwrap();
    let expected = store.read(&keys()).unwrap();
    let counts = (
        c.0.lock().unwrap().calls.len(),
        a.0.lock().unwrap().calls.len(),
    );
    assert!(store
        .compare_exchange(
            &keys(),
            &expected,
            &[Some(vec!["first".into()]), Some(vec!["second".into()])]
        )
        .is_err());
    assert_eq!(
        counts,
        (
            c.0.lock().unwrap().calls.len(),
            a.0.lock().unwrap().calls.len()
        )
    );
    assert!(store
        .compare_exchange(&keys(), &expected, &[None, None])
        .unwrap());
    assert!(a.0.lock().unwrap().values.is_empty());
    assert!(store
        .compare_exchange(&keys(), &expected, &[Some(vec!["following".into()]), None])
        .unwrap());
    assert_eq!(store.read(&keys()).unwrap()[0].entries, ["following"]);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn durability_requires_every_provider_without_sending_requests() {
    for mask in 0..8 {
        let (c, a, b, store) = fixture();
        for (bit, provider) in [&c, &a, &b].iter().enumerate() {
            provider.0.lock().unwrap().durable = mask & (1 << bit) != 0;
        }
        let counts: Vec<_> = [&c, &a, &b]
            .iter()
            .map(|p| p.0.lock().unwrap().calls.len())
            .collect();
        assert_eq!(store.supports_durable_flush(), mask == 7);
        assert_eq!(
            counts,
            [&c, &a, &b]
                .iter()
                .map(|p| p.0.lock().unwrap().calls.len())
                .collect::<Vec<_>>()
        );
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn failed_initialization_confirmation_preserves_one_acknowledged_decision() {
    let c = Store::default();
    let a = Store::default();
    let catalog = c.clone();
    c.on("swap", true, move || {
        catalog.on("scan", false, || {
            Err(error("initialization confirmation unavailable"))
        });
        Ok(())
    });
    assert!(Reclaiming::open(
        c.clone(),
        BTreeMap::from([("east".into(), a.clone())]),
        |_| "east".into(),
        Default::default()
    )
    .is_err());
    assert_eq!(c.count("swap"), 1);
    assert_eq!(c.0.lock().unwrap().values[EPOCH].entries, ["gc1", "0"]);
    let mut fresh = open(&c, &a, &Store::default(), Default::default());
    assert_eq!(
        fresh.read(&keys()).unwrap(),
        vec![JournalVersion::default(); 2]
    );
    assert_eq!(c.count("swap"), 1);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn failed_staging_confirmation_leaves_a_collectable_orphan_without_publication() {
    let (c, a, _, mut store) = fixture();
    let data = a.clone();
    a.on("swap", true, move || {
        data.on("read", false, || {
            Err(error("staging confirmation unavailable"))
        });
        Ok(())
    });
    let expected = store.read(&["a".into()]).unwrap();
    assert!(store
        .compare_exchange(&["a".into()], &expected, &[Some(vec!["orphan".into()])])
        .is_err());
    assert_eq!(a.count("swap"), 1);
    assert_eq!(a.0.lock().unwrap().values.len(), 1);
    assert_eq!(c.count("swap"), 1);
    assert!(!c.0.lock().unwrap().values.contains_key("a"));
    assert_eq!(store.read(&["a".into()]).unwrap(), expected);
    assert_eq!(store.collect_garbage().unwrap().unwrap().deleted, 1);
    assert!(a.0.lock().unwrap().values.is_empty());
    write(&mut store, "following", "b");
}
fn interrupted_refresh(phase: &str) {
    let (c, a, b, mut writer) = fixture();
    let mut reader = open(&c, &a, &b, Default::default());
    write(&mut writer, "old-a", "old-b");
    let provider = if phase == "catalog" {
        c.clone()
    } else {
        a.clone()
    };
    b.on("read", false, move || {
        write(&mut writer, "new-a", "new-b");
        assert_eq!(writer.collect_garbage()?.unwrap().deleted, 2);
        provider.on("read", false, || Err(error("refresh unavailable")));
        Ok(())
    });
    assert!(reader
        .read(&keys())
        .unwrap_err()
        .to_string()
        .contains("refresh unavailable"));
    assert_eq!(a.count("delete") + b.count("delete"), 2);
    let fresh = reader.read(&keys()).unwrap();
    assert_eq!(fresh[0].entries, ["new-a"]);
    assert_eq!(fresh[1].entries, ["new-b"]);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn interrupted_catalog_refresh_returns_no_partial_group() {
    interrupted_refresh("catalog");
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn interrupted_refreshed_object_returns_no_partial_group() {
    interrupted_refresh("data");
}
