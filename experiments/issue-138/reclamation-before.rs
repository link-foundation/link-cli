#![cfg(feature = "etcd")]
use link_cli::engine::{
    etcd::EtcdJournalStore,
    reclaiming::{ReclaimingPartitionedJournalStore, ReclamationOptions},
};
use std::collections::BTreeMap;
#[test]
fn invalid_reclamation_budget_rejects_before_network_io() {
    let provider = || EtcdJournalStore::new(vec!["http://127.0.0.1:1".into()], "/reclamation/".into()).unwrap();
    let result = ReclaimingPartitionedJournalStore::open(
        provider(), BTreeMap::from([("east".into(), provider())]),
        |_| "east".into(), ReclamationOptions { max_keys: 0, ..Default::default() },
    );
    assert!(result.is_err());
}
