use super::{harness::Harness, Store};
use link_cli::{
    engine::{
        cluster::{AtomicJournalStore, ClusterEngine},
        lsm::PhysicalLsmJournalStore,
        partitioned::PartitionedJournalStore,
        reclaiming::{ReclaimingPartitionedJournalStore, ReclamationOptions},
        EngineOptions,
    },
    LinkError, LinksStorage,
};
use std::collections::BTreeMap;
fn selected(h: &Harness) -> Result<Store, LinkError> {
    let mut options = super::options();
    options.limits.max_keys = 128;
    options.limits.max_bytes = 65536;
    options.limits.buffer_keys = 32;
    options.limits.buffer_bytes = 16384;
    options.limits.max_wal_bytes = 262144;
    PhysicalLsmJournalStore::open(h.wal()?, h.tables.clone(), options)
}
fn parts(h: &[Harness; 3]) -> Result<BTreeMap<String, Store>, LinkError> {
    Ok(BTreeMap::from([
        ("east".into(), selected(&h[1])?),
        ("west".into(), selected(&h[2])?),
    ]))
}
fn route(key: &str) -> String {
    if key == "a" { "east" } else { "west" }.into()
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn real_cluster_links_names_stale_guards_and_physical_reopen() -> Result<(), LinkError> {
    let h = Harness::new();
    {
        let cluster = ClusterEngine::<u32, _>::new(
            selected(&h)?,
            EngineOptions {
                require_durable_flush: true,
                ..Default::default()
            },
        )?;
        let mut seed = cluster.begin(&["a", "b"])?;
        seed.shard("a")?.create_link(0, 0)?;
        seed.shard("a")?.set_name(1, Some("雪".into()))?;
        seed.shard("b")?.create_link(0, 0)?;
        seed.commit()?;
        let mut stale = cluster.begin(&["a", "b"])?;
        let mut winner = cluster.begin(&["b"])?;
        winner.shard("b")?.set_name(1, Some("fresh".into()))?;
        winner.commit()?;
        stale.shard("a")?.delete_link(1)?;
        assert!(stale.commit().unwrap_err().to_string().contains("conflict"));
    }
    let fresh = Harness::from_image(h.image());
    {
        let mut store = selected(&fresh)?;
        store.compact()?;
        store.close()?;
    }
    let final_h = Harness::from_image(fresh.image());
    let cluster = ClusterEngine::<u32, _>::new(selected(&final_h)?, EngineOptions::default())?;
    let mut tx = cluster.begin(&["a", "b"])?;
    assert_eq!(tx.shard("a")?.get_name(1).as_deref(), Some("雪"));
    assert!(tx.shard("a")?.get_link(1).is_some());
    assert_eq!(tx.shard("b")?.get_name(1).as_deref(), Some("fresh"));
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn partitioned_catalogs_compose_actual_physical_stores_and_reopen() -> Result<(), LinkError> {
    let h = [Harness::new(), Harness::new(), Harness::new()];
    let keys = vec!["a".into(), "b".into()];
    {
        let mut s = PartitionedJournalStore::new(selected(&h[0])?, parts(&h)?, route)?;
        assert!(s.supports_durable_flush());
        let before = s.read(&keys)?;
        assert!(s.compare_exchange(&keys, &before, &[Some(vec!["雪".into()]), Some(vec![])])?);
        let stale = s.read(&keys)?;
        assert!(s.compare_exchange(&keys[..1], &stale[..1], &[Some(vec!["new".into()])])?);
        assert!(!s.compare_exchange(&keys, &stale, &[None, Some(vec!["lost".into()])])?);
    }
    let fresh = h.map(|h| Harness::from_image(h.image()));
    let mut s = PartitionedJournalStore::new(selected(&fresh[0])?, parts(&fresh)?, route)?;
    assert_eq!(
        s.read(&keys)?
            .iter()
            .map(|v| v.entries.clone())
            .collect::<Vec<_>>(),
        [vec!["new".to_owned()], vec![]]
    );
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn epoch_collection_uses_real_physical_scans_and_generation_tombstones() -> Result<(), LinkError> {
    let h = [Harness::new(), Harness::new(), Harness::new()];
    let keys = vec!["a".into(), "b".into()];
    {
        let mut s = ReclaimingPartitionedJournalStore::open(
            selected(&h[0])?,
            parts(&h)?,
            route,
            ReclamationOptions::default(),
        )?;
        assert!(s.supports_durable_flush());
        for i in 0..2 {
            let before = s.read(&keys)?;
            assert!(s.compare_exchange(
                &keys,
                &before,
                &[Some(vec![format!("a{i}")]), Some(vec![format!("b{i}")])]
            )?);
        }
        let result = s.collect_garbage()?.expect("collection result");
        assert_eq!((result.epoch, result.examined, result.deleted), (1, 4, 2));
    }
    let fresh = h.map(|h| Harness::from_image(h.image()));
    let mut s = ReclaimingPartitionedJournalStore::open(
        selected(&fresh[0])?,
        parts(&fresh)?,
        route,
        ReclamationOptions::default(),
    )?;
    assert_eq!(
        s.read(&keys)?
            .iter()
            .map(|v| v.entries.clone())
            .collect::<Vec<_>>(),
        [vec!["a1".to_owned()], vec!["b1".to_owned()]]
    );
    assert_eq!(s.collect_garbage()?.expect("collection result").deleted, 0);
    Ok(())
}
