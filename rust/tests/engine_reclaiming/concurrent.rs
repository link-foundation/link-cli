use super::*;
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn concurrent_publication_at_each_collection_seam_remains_reachable() {
    for phase in ["catalog-scan", "east-scan", "west-scan", "delete"] {
        let (c, a, b, mut writer) = fixture();
        let mut collector = open(&c, &a, &b, Default::default());
        write(&mut writer, "old", "b0");
        write(&mut writer, "current", "b1");
        let (provider, operation) = match phase {
            "catalog-scan" => (&c, "scan"),
            "east-scan" => (&a, "scan"),
            "west-scan" => (&b, "scan"),
            _ => (&a, "delete"),
        };
        let mut following = open(&c, &a, &b, Default::default());
        provider.on(operation, false, move || {
            write(&mut following, "concurrent", "b2");
            Ok(())
        });
        assert_eq!(collector.collect_garbage().unwrap().unwrap().epoch, 1);
        assert_eq!(writer.read(&keys()).unwrap()[0].entries, ["concurrent"]);
        writer.collect_garbage().unwrap();
        assert_eq!(writer.read(&keys()).unwrap()[1].entries, ["b2"]);
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn epoch_fence_just_before_publication_returns_known_conflict() {
    let (c, a, b, mut writer) = fixture();
    let mut collector = open(&c, &a, &b, Default::default());
    write(&mut writer, "old", "b");
    c.on("swap", false, move || {
        collector.collect_garbage()?;
        Ok(())
    });
    let before = writer.read(&keys()).unwrap();
    assert!(!writer
        .compare_exchange(&keys(), &before, &[Some(vec!["in-flight".into()]), None])
        .unwrap());
    assert_eq!(writer.read(&keys()).unwrap()[0].entries, ["old"]);
    write(&mut writer, "following", "b");
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn own_fence_excludes_newer_orphans_after_an_interrupted_competing_collection() {
    let (c, a, b, mut writer) = fixture();
    let mut collector = open(&c, &a, &b, Default::default());
    let mut other = open(&c, &a, &b, Default::default());
    write(&mut writer, "live", "b");
    let catalog = c.clone();
    let data = a.clone();
    c.on("scan", false, move || {
        catalog.on("swap", false, || Err(error("lost publication")));
        let key = vec!["a".into()];
        let before = writer.read(&key)?;
        assert!(writer
            .compare_exchange(&key, &before, &[Some(vec!["orphan at one".into()])])
            .is_err());
        data.on("scan", false, || Err(error("incomplete next collector")));
        assert!(other.collect_garbage().is_err());
        Ok(())
    });
    let result = collector.collect_garbage().unwrap().unwrap();
    assert_eq!((result.epoch, result.examined, result.deleted), (1, 3, 0));
    assert!(a
        .0
        .lock()
        .unwrap()
        .values
        .keys()
        .any(|key| key.starts_with("blob2-0000000000000001-")));
    assert_eq!(collector.collect_garbage().unwrap().unwrap().deleted, 1);
    assert_eq!(collector.read(&keys()).unwrap()[0].entries, ["live"]);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_second_retirement_stops_after_one_group_retry() {
    let (c, a, b, mut writer) = fixture();
    let mut reader = open(&c, &a, &b, Default::default());
    write(&mut writer, "zero", "b");
    let data = a.clone();
    let mut second = open(&c, &a, &b, Default::default());
    a.on("read", false, move || {
        write(&mut writer, "one", "b");
        writer.collect_garbage()?;
        data.on("read", false, move || {
            write(&mut second, "two", "b");
            second.collect_garbage()?;
            Ok(())
        });
        Ok(())
    });
    assert!(reader.read(&keys()).is_err());
    assert_eq!(reader.read(&keys()).unwrap()[0].entries, ["two"]);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn resurrection_stages_a_new_epoch_and_old_owned_values_cannot_publish() {
    let (c, a, _, mut writer) = fixture();
    let old = write(&mut writer, "old", "b");
    let old_key = c.0.lock().unwrap().values["a"].entries[2].clone();
    write(&mut writer, "new", "b");
    assert_eq!(writer.collect_garbage().unwrap().unwrap().deleted, 1);
    assert!(!a.0.lock().unwrap().values.contains_key(&old_key));
    assert!(!writer
        .compare_exchange(&keys(), &old, &[Some(vec!["stale".into()]), None])
        .unwrap());
    write(&mut writer, "old", "b");
    let new_key = c.0.lock().unwrap().values["a"].entries[2].clone();
    assert_ne!(old_key, new_key);
    assert!(new_key.starts_with("blob2-0000000000000001-"));
    assert_eq!(old[0].entries, ["old"]);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn deletion_response_loss_before_and_after_apply_is_never_replayed() {
    for after in [false, true] {
        let (_, a, _, mut store) = fixture();
        write(&mut store, "old", "b");
        write(&mut store, "new", "b");
        a.on("delete", after, || Err(error("delete response lost")));
        assert!(store.collect_garbage().is_err());
        assert_eq!(a.count("delete"), 1);
        assert_eq!(a.0.lock().unwrap().values.len(), if after { 1 } else { 2 });
        assert_eq!(store.read(&keys()).unwrap()[0].entries, ["new"]);
        assert_eq!(
            store.collect_garbage().unwrap().unwrap().deleted,
            usize::from(!after)
        );
    }
}
