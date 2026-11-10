#![cfg(feature = "etcd")]
use link_cli::engine::{etcd::EtcdJournalStore, maintenance::{JournalMaintenanceStore, JournalScanOptions}};
#[test]
fn namespace_options_fail_before_io() {
    let mut store = EtcdJournalStore::new(vec!["http://127.0.0.1:1".into()], "/test/".into()).unwrap();
    assert!(store.scan_namespace(JournalScanOptions { max_keys: 0, ..Default::default() }).is_err());
    assert!(store.compare_delete("a", 0).is_err());
}
