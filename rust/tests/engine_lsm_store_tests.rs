#![cfg(feature = "physical-lsm")]
// Production controls retain the public behaviors of the pre-implementation fixture.
use link_cli::engine::cluster::{AtomicJournalStore, JournalVersion};
use link_cli::engine::lsm::{LsmOptions, LsmStoreOptions, PhysicalLsmJournalStore};
use link_cli::engine::maintenance::{JournalMaintenanceStore, JournalScanOptions};
use link_cli::engine::paged::CheckpointPagedLog;
use link_cli::{LinkError, TransitionLogStore};
#[path = "common/lsm_store.rs"]
mod harness;
use harness::{Device, Harness, Tables};
fn options() -> LsmStoreOptions {
    LsmStoreOptions {
        limits: LsmOptions {
            max_keys: 32,
            max_bytes: 4096,
            buffer_keys: 2,
            buffer_bytes: 128,
            max_runs: 2,
            max_wal_records: 4,
            max_wal_bytes: 8192,
        },
        max_namespace_entries: 16,
    }
}
fn put<S: AtomicJournalStore>(
    store: &mut S,
    key: &str,
    entries: Vec<String>,
) -> Result<(), LinkError> {
    let keys = vec![key.to_owned()];
    let before = store.read(&keys)?;
    assert!(store.compare_exchange(&keys, &before, &[Some(entries)])?);
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn physical_lsm_group_owned_tombstone_empty_and_reopen() -> Result<(), LinkError> {
    let devices = [Device::default(), Device::default(), Device::default()];
    let wal = CheckpointPagedLog::with_max_pages(
        devices[0].clone(),
        [devices[1].clone(), devices[2].clone()],
        128,
    )?;
    let tables = Tables::default();
    let mut store = PhysicalLsmJournalStore::open(wal, tables.clone(), options())?;
    let keys = vec!["a".into(), "b".into()];
    let absent = store.read(&keys)?;
    assert_eq!(absent, vec![JournalVersion::default(); 2]);
    assert!(store.compare_exchange(
        &keys,
        &absent,
        &[Some(vec!["\u{feff}雪\0😀\n".into()]), Some(vec![])]
    )?);
    let mut first = store.read(&keys)?;
    assert!(first[0].generation > 0);
    assert_eq!(first[0].generation, first[1].generation);
    first[0].entries[0] = "caller".into();
    assert_eq!(store.read(&keys)?[0].entries[0], "\u{feff}雪\0😀\n");
    store.flush()?;
    let count = tables.0.borrow().mutations.len();
    assert!(!store.compare_exchange(&keys, &absent, &[None, Some(vec!["lost".into()])])?);
    let current = store.read(&keys)?;
    assert!(store.compare_exchange(
        &keys,
        &current,
        &[Some(current[0].entries.clone()), Some(vec![])]
    )?);
    assert_eq!(tables.0.borrow().mutations.len(), count);
    assert!(store.compare_delete("a", current[0].generation)?);
    let deleted = store.read(&keys)?[0].clone();
    assert!(deleted.generation > current[0].generation);
    assert!(deleted.entries.is_empty());
    assert!(!store.compare_delete("a", deleted.generation)?);
    assert_eq!(
        store
            .scan_namespace(JournalScanOptions::default())?
            .iter()
            .map(|r| r.key.as_str())
            .collect::<Vec<_>>(),
        ["b"]
    );
    put(&mut store, "a", vec![])?;
    store.compact()?;
    let expected = store.read(&keys)?;
    store.close()?;
    assert!(tables.0.borrow().leases.is_empty());
    let fresh = Tables::default();
    for (id, device) in &tables.0.borrow().files {
        fresh.0.borrow_mut().files.insert(*id, device.crash());
    }
    let wal = CheckpointPagedLog::with_max_pages(
        devices[0].crash(),
        [devices[1].crash(), devices[2].crash()],
        128,
    )?;
    let mut reopened = PhysicalLsmJournalStore::open(wal, fresh, options())?;
    assert_eq!(reopened.read(&keys)?, expected);
    reopened.close()?;
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn physical_lsm_repeated_buffer_and_wal_compaction() -> Result<(), LinkError> {
    let wal = CheckpointPagedLog::with_max_pages(
        Device::default(),
        [Device::default(), Device::default()],
        128,
    )?;
    let tables = Tables::default();
    let mut store = PhysicalLsmJournalStore::open(wal, tables.clone(), options())?;
    for i in 0..30 {
        put(&mut store, &format!("k{}", i % 5), vec![i.to_string()])?;
        if i % 4 == 0 {
            store.flush()?;
        }
    }
    store.compact()?;
    assert_eq!(tables.0.borrow().files.len(), 1);
    assert!(tables.0.borrow().mutations.iter().any(|s| s == "delete"));
    assert_eq!(store.read(&["k2".into()])?[0].entries, ["27"]);
    assert!(put(&mut store, "oversized", vec!["x".repeat(129)]).is_err());
    put(&mut store, "following", vec!["ok".into()])?;
    store.flush()?;
    let (mut wal, _) = store.into_parts()?;
    assert!(wal.read_log_entries()?.len() <= 4);
    Ok(())
}

type Store = PhysicalLsmJournalStore<CheckpointPagedLog<Device, Device>, Tables>;
fn open(harness: &Harness) -> Result<Store, LinkError> {
    PhysicalLsmJournalStore::open(harness.wal()?, harness.tables.clone(), options())
}
fn prepared() -> Result<(Harness, Store, Vec<JournalVersion>), LinkError> {
    let harness = Harness::new();
    let mut store = open(&harness)?;
    put(&mut store, "a", vec!["A".into()])?;
    put(&mut store, "b", vec![])?;
    store.flush()?;
    put(&mut store, "a", vec!["new".into()])?;
    let generation = store.read(&["b".into()])?[0].generation;
    assert!(store.compare_delete("b", generation)?);
    store.flush()?;
    put(&mut store, "c", vec!["C".into()])?;
    let before = store.read(&["a".into(), "b".into(), "c".into(), "d".into()])?;
    Ok((harness, store, before))
}
fn change(store: &mut Store, before: &[JournalVersion]) -> Result<bool, LinkError> {
    store.compare_exchange(
        &["a".into(), "d".into()],
        &[before[0].clone(), before[3].clone()],
        &[Some(vec!["changed".into()]), Some(vec!["D".into()])],
    )
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_compaction_mutation_cut_preserves_acknowledged_values_and_following_use(
) -> Result<(), LinkError> {
    let (h, mut s, _) = prepared()?;
    h.reset();
    s.compact()?;
    let trace = h.trace();
    s.close()?;
    assert!(trace.iter().any(|name| name.starts_with("create:")));
    assert!(trace.iter().any(|name| name.starts_with("delete:")));
    for at in 1..=trace.len() {
        for lost_reply in [false, true] {
            let (h, mut s, before) = prepared()?;
            h.reset();
            h.fault(at, lost_reply);
            assert!(s.compact().unwrap_err().to_string().contains("injected"));
            let count = h.trace().len();
            assert!(s
                .read(&["a".into()])
                .unwrap_err()
                .to_string()
                .contains("fenced"));
            assert_eq!(h.trace().len(), count);
            let fresh = Harness::from_image(h.image());
            let mut reopened = open(&fresh)?;
            assert_eq!(
                reopened.read(&["a".into(), "b".into(), "c".into(), "d".into()])?,
                before,
                "{at}/{lost_reply}/{}",
                trace[at - 1]
            );
            let reserved = link_cli::engine::lsm::LsmWalState::recover(
                &fresh.wal()?.read_log_entries()?,
                options().limits,
            )?
            .last_reserved_table_id;
            fresh.reset();
            put(&mut reopened, "following", vec!["ok".into()])?;
            reopened.flush()?;
            for name in fresh
                .trace()
                .iter()
                .filter_map(|name| name.strip_prefix("create:").map(str::to_owned))
            {
                assert!(name.parse::<u64>().unwrap() > reserved);
            }
            reopened.close()?;
            s.close()?;
        }
    }
    println!(
        "compaction: {} effects; {} finite before/after cuts",
        trace.len(),
        trace.len() * 2
    );
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_group_write_mutation_cut_recovers_whole_old_or_new_group() -> Result<(), LinkError> {
    let (h, mut s, before) = prepared()?;
    h.reset();
    assert!(change(&mut s, &before)?);
    let trace = h.trace();
    let keys = ["a".into(), "b".into(), "c".into(), "d".into()];
    let after = s.read(&keys)?;
    s.close()?;
    assert!(trace.iter().any(|name| name.starts_with("create:")));
    assert!(trace.iter().any(|name| name.starts_with("delete:")));
    for at in 1..=trace.len() {
        for lost_reply in [false, true] {
            let (h, mut s, before) = prepared()?;
            h.reset();
            h.fault(at, lost_reply);
            assert!(change(&mut s, &before)
                .unwrap_err()
                .to_string()
                .contains("injected"));
            let count = h.trace().len();
            assert!(s.read(&keys).is_err());
            assert_eq!(h.trace().len(), count);
            let fresh = Harness::from_image(h.image());
            let mut reopened = open(&fresh)?;
            let actual = reopened.read(&keys)?;
            assert!(
                actual == before || actual == after,
                "{at}/{lost_reply}/{}",
                trace[at - 1]
            );
            put(&mut reopened, "following", vec!["ok".into()])?;
            reopened.compact()?;
            let image = fresh.image();
            reopened.close()?;
            s.close()?;
            let next = Harness::from_image(image);
            let mut following = open(&next)?;
            assert_eq!(following.read(&["following".into()])?[0].entries, ["ok"]);
            following.close()?;
        }
    }
    println!(
        "group: {} effects; {} finite before/after cuts",
        trace.len(),
        trace.len() * 2
    );
    Ok(())
}

fn sealed() -> Result<harness::Snapshot, LinkError> {
    let h = Harness::new();
    let mut s = open(&h)?;
    for key in ["a", "b"] {
        put(&mut s, key, vec![key.into()])?;
        s.flush()?;
    }
    let image = h.image();
    s.close()?;
    Ok(image)
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn all_selected_bindings_validate_before_any_repair_completes() -> Result<(), LinkError> {
    use link_cli::{engine::lsm::LsmWalState, ReplaceableTransitionLogStore};
    let image = sealed()?;
    let h = Harness::from_image(image.clone());
    let mut wal = h.wal()?;
    let mut state = LsmWalState::recover(&wal.read_log_entries()?, options().limits)?;
    state.manifest[1].digest ^= 1;
    wal.replace_log_entries(&[state.encode(options().limits)?])?;
    let before = h.image();
    h.reset();
    let error = PhysicalLsmJournalStore::open(wal, h.tables.clone(), options())
        .err()
        .unwrap();
    assert!(error.to_string().contains("binding changed"));
    assert_eq!(h.image(), before);
    assert!(h.tables.0.borrow().leases.is_empty());
    for device in h.tables.0.borrow().files.values() {
        assert_eq!(device.0.borrow().completed, 0);
    }
    let good = Harness::from_image(image);
    let mut s = open(&good)?;
    for device in good.tables.0.borrow().files.values() {
        assert_eq!(device.0.borrow().completed, 1);
    }
    assert_eq!(
        s.scan_namespace(JournalScanOptions::default())?
            .iter()
            .map(|row| row.key.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    s.close()?;
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn close_failure_retains_cleanup_ownership_for_explicit_retry() -> Result<(), LinkError> {
    let h = Harness::from_image(sealed()?);
    let mut s = open(&h)?;
    h.reset();
    h.fault(1, false);
    assert!(s.close().unwrap_err().to_string().contains("injected"));
    assert_eq!(h.tables.0.borrow().leases.len(), 1);
    assert!(s.read(&["a".into()]).is_err());
    h.reset();
    s.close()?;
    assert!(h.tables.0.borrow().leases.is_empty());
    s.close()?;
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn durable_namespace_rejects_a_downgraded_selected_device_without_mutation() -> Result<(), LinkError>
{
    let h = Harness::from_image(sealed()?);
    h.tables
        .0
        .borrow()
        .files
        .values()
        .next()
        .unwrap()
        .0
        .borrow_mut()
        .non_durable = true;
    let before = h.image();
    h.reset();
    let error = open(&h).err().unwrap();
    assert!(error.to_string().contains("non-durable"));
    assert_eq!(h.image(), before);
    assert!(h.tables.0.borrow().leases.is_empty());
    assert!(h.trace().iter().all(|name| name.starts_with("close:")));
    Ok(())
}

#[path = "common/lsm_consumers.rs"]
mod consumers;
#[path = "common/lsm_contract.rs"]
mod contract;
#[path = "common/lsm_mirror.rs"]
mod mirror;
#[path = "common/lsm_selection.rs"]
mod selection;
