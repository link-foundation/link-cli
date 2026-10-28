use super::{harness::Harness, options, put};
use link_cli::{
    engine::{
        cluster::AtomicJournalStore,
        lsm::PhysicalLsmJournalStore,
        maintenance::{JournalMaintenanceStore, JournalScanOptions},
    },
    LinkError, TransitionLogStore,
};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_groups_and_bounded_owned_scans_preserve_durable_image() -> Result<(), LinkError> {
    let h = Harness::new();
    let mut s = PhysicalLsmJournalStore::open(h.wal()?, h.tables.clone(), options())?;
    h.reset();
    let before = h.image();
    for keys in [
        vec![],
        vec!["a".into(); 33],
        vec!["a".into(), "a".into()],
        vec!["".into()],
        vec!["雪".into()],
        vec!["a".repeat(129)],
    ] {
        assert!(s.read(&keys).is_err());
    }
    assert!(s.compare_exchange(&["a".into()], &[], &[]).is_err());
    assert!(s.compare_delete("a", 0).is_err());
    assert!(!s.compare_delete("missing", 1)?);
    assert!(h.trace().is_empty());
    assert_eq!(before, h.image());
    put(&mut s, "a", vec!["A".into()])?;
    put(&mut s, "b", vec![])?;
    s.flush()?;
    assert!(s
        .scan_namespace(JournalScanOptions {
            keys_only: true,
            ..Default::default()
        })?
        .iter()
        .all(|r| r.version.entries.is_empty()));
    for selected in [
        JournalScanOptions {
            max_keys: 1,
            ..Default::default()
        },
        JournalScanOptions {
            max_bytes: 9,
            ..Default::default()
        },
        JournalScanOptions {
            page_keys: 0,
            ..Default::default()
        },
    ] {
        assert!(s.scan_namespace(selected).is_err());
    }
    let mut rows = s.scan_namespace(Default::default())?;
    rows[0].version.entries[0] = "caller".into();
    assert_eq!(s.read(&["a".into()])?[0].entries, ["A"]);
    s.close()?;
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn group_capacity_and_complete_namespace_admission_precede_effects() -> Result<(), LinkError> {
    for mode in 0..3 {
        let h = Harness::new();
        let mut selected = options();
        match mode {
            0 => selected.limits.max_wal_records = 1,
            1 => {
                selected.limits.max_keys = 1;
                selected.limits.buffer_keys = 1;
            }
            _ => {
                selected.limits.max_bytes = 16;
                selected.limits.buffer_bytes = 16;
            }
        }
        let mut s = PhysicalLsmJournalStore::open(h.wal()?, h.tables.clone(), selected)?;
        if mode == 1 {
            put(&mut s, "a", vec![])?;
        }
        h.reset();
        let before = h.image();
        assert!(put(
            &mut s,
            if mode == 1 { "b" } else { "a" },
            if mode == 2 {
                vec!["x".repeat(8)]
            } else {
                vec![]
            }
        )
        .is_err());
        assert!(h.trace().is_empty());
        assert_eq!(before, h.image());
        s.close()?;
    }
    let h = Harness::new();
    let mut selected = options();
    selected.max_namespace_entries = 2;
    selected.limits.max_runs = 4;
    let mut s = PhysicalLsmJournalStore::open(h.wal()?, h.tables.clone(), selected)?;
    for key in ["a", "b"] {
        put(&mut s, key, vec![])?;
        s.flush()?;
    }
    put(&mut s, "c", vec![])?;
    h.reset();
    let before = h.image();
    assert!(s.flush().unwrap_err().to_string().contains("namespace"));
    assert!(h.trace().is_empty());
    assert_eq!(before, h.image());
    s.close()?;
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn hot_key_replay_rotates_record_and_byte_bounds_without_table_publication() -> Result<(), LinkError>
{
    for bytes in [false, true] {
        let h = Harness::new();
        let mut selected = options();
        selected.limits.buffer_keys = 32;
        selected.limits.max_wal_records = if bytes { 100 } else { 2 };
        selected.limits.max_wal_bytes = if bytes { 180 } else { 8192 };
        let mut s = PhysicalLsmJournalStore::open(h.wal()?, h.tables.clone(), selected)?;
        for i in 0..12 {
            put(&mut s, "a", vec![i.to_string()])?;
        }
        assert!(h.tables.0.borrow().files.is_empty());
        s.close()?;
        assert!(h.wal()?.read_log_entries()?.len() < 12);
        let fresh = Harness::from_image(h.image());
        let mut again =
            PhysicalLsmJournalStore::open(fresh.wal()?, fresh.tables.clone(), selected)?;
        assert_eq!(again.read(&["a".into()])?[0].entries, ["11"]);
        again.close()?;
    }
    Ok(())
}
