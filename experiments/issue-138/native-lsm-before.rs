use link_cli::engine::{cluster::AtomicJournalStore, lsm::{FileLsmJournalStore, FileLsmOptions}};
#[test]
fn native_lsm_owns_files_and_reopens_acknowledged_journals() {
    let directory = tempfile::tempdir().unwrap();
    let options = FileLsmOptions { max_pages: 128, ..Default::default() };
    let keys = vec!["a".into(), "b".into()];
    let mut store = FileLsmJournalStore::open(directory.path(), options).unwrap();
    let before = store.read(&keys).unwrap();
    assert!(store.compare_exchange(&keys, &before, &[Some(vec!["durable\0雪😀".into()]), Some(vec![])]).unwrap());
    store.flush().unwrap();
    assert!(FileLsmJournalStore::open(directory.path(), options).is_err());
    store.close().unwrap();
    let mut reopened = FileLsmJournalStore::open(directory.path(), options).unwrap();
    let values = reopened.read(&keys).unwrap();
    assert_eq!(values[0].entries, ["durable\0雪😀"]);
    assert!(values[1].entries.is_empty());
    reopened.close().unwrap();
}
