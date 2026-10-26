//! Selected coordinator maintenance; use a fresh dedicated namespace.
use link_cli::engine::{
    cluster::{AtomicJournalStore, JournalVersion},
    etcd::EtcdJournalStore,
    maintenance::{JournalMaintenanceStore, JournalScanOptions},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: journal_maintenance endpoints-comma-separated fresh-prefix/".into());
    }
    let mut store = EtcdJournalStore::new(
        args[1].split(',').map(String::from).collect(),
        args[2].clone(),
    )?;
    let options = JournalScanOptions {
        page_keys: 1,
        ..Default::default()
    };
    if !store.scan_namespace(options)?.is_empty() {
        return Err("example requires an empty dedicated namespace".into());
    }
    let keys = vec!["a".into(), "b".into()];
    assert!(store.compare_exchange(
        &keys,
        &[JournalVersion::default(), JournalVersion::default()],
        &[Some(vec!["first 雪".into()]), Some(vec!["second".into()])]
    )?);
    let before = store.scan_namespace(options)?;
    assert_eq!(before.len(), 2);
    assert_eq!(before[0].key, "a");
    assert_eq!(before[0].version.entries, ["first 雪"]);
    assert!(store.compare_delete("a", before[0].version.generation)?);
    assert!(!store.compare_delete("a", before[0].version.generation)?);
    assert!(store.compare_exchange(
        &["a".into()],
        &[JournalVersion::default()],
        &[Some(vec!["replacement".into()])]
    )?);
    assert!(!store.compare_delete("a", before[0].version.generation)?);
    let after = store.scan_namespace(options)?;
    assert!(after[0].version.generation > before[0].version.generation);
    assert_eq!(after[0].version.entries, ["replacement"]);
    assert_eq!(after[1], before[1]);
    let keys_only = store.scan_namespace(JournalScanOptions {
        keys_only: true,
        ..options
    })?;
    assert!(keys_only
        .iter()
        .all(|entry| entry.version.entries.is_empty()));
    assert_eq!(keys_only[0].version.generation, after[0].version.generation);
    println!("PASS Rust: pinned pagination, conditional delete, absent conflict, ABA protection, keys-only scan");
    Ok(())
}
