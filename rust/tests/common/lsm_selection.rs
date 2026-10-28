use super::{harness::Harness, open, options, put};
use link_cli::{
    engine::{
        cluster::AtomicJournalStore,
        lsm::{
            LsmStoreOptions, LsmTableDescriptor, LsmTableNamespace, LsmWalState,
            PhysicalLsmJournalStore,
        },
        paged::{SortedJournalEntry, SortedJournalTable, SortedTableOptions},
    },
    LinkError, ReplaceableTransitionLogStore,
};
fn row(key: &str, generation: u64, value: Option<&str>) -> SortedJournalEntry {
    SortedJournalEntry {
        key: key.into(),
        generation,
        entries: value.map(|v| vec![v.into()]),
    }
}
fn retained(
    rows: Vec<Vec<SortedJournalEntry>>,
    generation: u64,
    options: LsmStoreOptions,
) -> Result<Harness, LinkError> {
    let h = Harness::new();
    let mut provider = h.tables.clone();
    let mut manifest = vec![];
    for records in rows {
        let id = manifest.len() as u64 + 1;
        let device = provider.create(id)?;
        let table = SortedJournalTable::create(device, &records, SortedTableOptions::default())?;
        manifest.push(LsmTableDescriptor {
            id,
            identity: table
                .identity()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            digest: table.digest(),
            count: table.len(),
            stored_bytes: table.stored_bytes(),
        });
        let mut device = table.into_device();
        provider.close(&mut device)?;
    }
    let state = LsmWalState {
        generation,
        covered_generation: generation,
        last_reserved_table_id: manifest.len() as u64,
        manifest,
        buffer: vec![],
    };
    h.wal()?
        .replace_log_entries(&[state.encode(options.limits)?])?;
    Ok(Harness::from_image(h.image()))
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn physical_generations_and_all_historical_duplicates_validate_before_cleanup(
) -> Result<(), LinkError> {
    for (rows, generation, message) in [
        (vec![vec![row("a", 2, None)]], 1, "exceeds coverage"),
        (
            vec![vec![row("a", 1, None)]],
            2,
            "missing global generation",
        ),
        (
            vec![
                vec![row("a", 1, Some("old"))],
                vec![row("a", 2, Some("new"))],
                vec![row("a", 1, Some("conflict"))],
            ],
            2,
            "conflicting table versions",
        ),
    ] {
        let mut opts = options();
        opts.limits.max_runs = 4;
        let h = retained(rows, generation, opts)?;
        let before = h.image();
        let error = PhysicalLsmJournalStore::open(h.wal()?, h.tables.clone(), opts)
            .err()
            .unwrap();
        assert!(error.to_string().contains(message), "{error}");
        assert_eq!(h.image(), before);
        assert!(h.tables.0.borrow().leases.is_empty());
        assert!(h.trace().iter().all(|name| name.starts_with("close:")));
        for d in h.tables.0.borrow().files.values() {
            assert_eq!(d.0.borrow().completed, 0);
        }
    }
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn retained_combined_key_and_byte_bounds_precede_any_cleanup() -> Result<(), LinkError> {
    for bytes in [false, true] {
        let mut opts = options();
        if bytes {
            opts.limits.max_bytes = 20;
            opts.limits.buffer_bytes = 20;
        } else {
            opts.limits.max_keys = 1;
            opts.limits.buffer_keys = 1;
        }
        let h = retained(
            vec![vec![row("a", 1, Some("A"))], vec![row("b", 2, Some("B"))]],
            2,
            opts,
        )?;
        let before = h.image();
        let error = PhysicalLsmJournalStore::open(h.wal()?, h.tables.clone(), opts)
            .err()
            .unwrap();
        assert!(error.to_string().contains("retained capacity"));
        assert_eq!(h.image(), before);
        assert!(h.tables.0.borrow().leases.is_empty());
    }
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn changed_generation_exhaustion_rejects_while_exact_noops_remain_possible() -> Result<(), LinkError>
{
    let h = retained(
        vec![vec![row("a", u64::MAX, Some("A"))]],
        u64::MAX,
        options(),
    )?;
    let mut s = open(&h)?;
    let before = s.read(&["a".into()])?;
    h.reset();
    let image = h.image();
    assert!(s.compare_exchange(&["a".into()], &before, &[Some(vec!["A".into()])])?);
    assert!(put(&mut s, "a", vec!["changed".into()])
        .unwrap_err()
        .to_string()
        .contains("exhausted"));
    assert!(h.trace().is_empty());
    assert_eq!(h.image(), image);
    s.close()?;
    Ok(())
}
