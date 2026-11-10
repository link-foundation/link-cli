//! Failure-injection regressions for the transaction durability boundary.
use link_cli::{
    CommitMode, FileTransitionLog, GenericTransactionsDecorator, LinkStorage, LogRetentionPolicy,
};
use tempfile::tempdir;

#[test]
fn committed_buffered_store_survives_reopen_without_save() {
    let directory = tempdir().unwrap();
    let data = directory.path().join("data.links");
    let log = directory.path().join("wal");
    let open = || {
        GenericTransactionsDecorator::new(
            LinkStorage::new(&data, false).unwrap(),
            FileTransitionLog::open(&log).unwrap(),
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap()
    };
    let id = {
        let mut tx = open();
        tx.begin_transaction().unwrap();
        let id = tx.create(0, 0).unwrap();
        tx.commit().unwrap();
        id
    };
    assert!(
        open().exists(id),
        "commit must persist data before writing applied markers"
    );
}

#[test]
fn rollback_is_persistent_before_its_marker() {
    let directory = tempdir().unwrap();
    let data = directory.path().join("data.links");
    let log = directory.path().join("wal");
    let open = || {
        GenericTransactionsDecorator::new(
            LinkStorage::new(&data, false).unwrap(),
            FileTransitionLog::open(&log).unwrap(),
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap()
    };
    let id = {
        let mut tx = open();
        tx.begin_transaction().unwrap();
        let id = tx.create(0, 0).unwrap();
        tx.save().unwrap();
        tx.rollback().unwrap();
        id
    };
    assert!(
        !open().exists(id),
        "rollback marker must never outlive an unflushed undo"
    );
}

#[derive(Default)]
struct MemoryLog(Vec<String>);
impl link_cli::TransitionLogStore for MemoryLog {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), link_cli::LinkError> {
        self.0.push(entry.into());
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, link_cli::LinkError> {
        Ok(self.0.clone())
    }
    fn flush_log(&mut self) -> Result<(), link_cli::LinkError> {
        Ok(())
    }
}

#[test]
fn recovery_refuses_a_complete_corrupt_transition() {
    let log = MemoryLog(vec!["__transactions:transition:garbage".into()]);
    let recovered = GenericTransactionsDecorator::new(
        LinkStorage::in_memory(false),
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    );
    assert!(
        recovered.is_err(),
        "a complete corrupt entry must not silently erase a committed write"
    );
}

#[test]
fn recovery_refuses_corrupt_markers() {
    for suffix in [
        "commit:bad-id",
        "rollback:bad-id",
        "applied:bad",
        "applied:0",
    ] {
        let log = MemoryLog(vec![format!("__transactions:{suffix}")]);
        assert!(
            GenericTransactionsDecorator::new(
                LinkStorage::in_memory(false),
                log,
                LogRetentionPolicy::Infinite,
                CommitMode::Sync,
                false,
            )
            .is_err(),
            "accepted {suffix}"
        );
    }
}

struct FailingStore {
    inner: LinkStorage,
    fail_flush: std::rc::Rc<std::cell::Cell<bool>>,
    fail_update: bool,
}
impl link_cli::LinksStorage<u32> for FailingStore {
    fn create_link(&mut self, s: u32, t: u32) -> Result<u32, link_cli::LinkError> {
        Ok(self.inner.create(s, t))
    }
    fn ensure_link_created(&mut self, id: u32) -> Result<u32, link_cli::LinkError> {
        Ok(self.inner.ensure_created(id))
    }
    fn get_link(&self, id: u32) -> Option<link_cli::Link> {
        self.inner.get(id).copied()
    }
    fn update_link(
        &mut self,
        id: u32,
        s: u32,
        t: u32,
    ) -> Result<link_cli::Link, link_cli::LinkError> {
        if self.fail_update {
            return Err(link_cli::LinkError::Transaction(
                "injected update failure".into(),
            ));
        }
        link_cli::LinksStorage::update_link(&mut self.inner, id, s, t)
    }
    fn delete_link(&mut self, id: u32) -> Result<link_cli::Link, link_cli::LinkError> {
        link_cli::LinksStorage::delete_link(&mut self.inner, id)
    }
    fn all_links(&self) -> Vec<link_cli::Link> {
        self.inner.all().into_iter().copied().collect()
    }
    fn flush(&mut self) -> Result<(), link_cli::LinkError> {
        if self.fail_flush.get() {
            return Err(link_cli::LinkError::Transaction(
                "injected flush failure".into(),
            ));
        }
        Ok(())
    }
}

#[test]
fn failed_checkpoint_never_writes_applied_markers() {
    let failing = std::rc::Rc::new(std::cell::Cell::new(false));
    let store = FailingStore {
        inner: LinkStorage::in_memory(false),
        fail_flush: failing.clone(),
        fail_update: false,
    };
    let mut tx = GenericTransactionsDecorator::new(
        store,
        MemoryLog::default(),
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap();
    tx.begin_transaction().unwrap();
    tx.create(0, 0).unwrap();
    failing.set(true);
    assert!(tx.commit().is_err());
    assert_eq!(tx.applied_sequence(), 0);
    assert!(!tx
        .log_store()
        .0
        .iter()
        .any(|e| e.starts_with("__transactions:applied:")));
    let (_, log) = tx.into_inner();
    let reopened = GenericTransactionsDecorator::new(
        LinkStorage::in_memory(false),
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap();
    assert!(
        reopened.exists(1),
        "durable commit must be recovered after checkpoint failure"
    );
}

#[test]
fn failed_recovery_is_reported() {
    let store = FailingStore {
        inner: LinkStorage::in_memory(false),
        fail_flush: Default::default(),
        fail_update: true,
    };
    let log = MemoryLog(vec![
        "__transactions:transition:v1|00000000000000000000000000000001|1|0|0|0,0,0|1,0,0".into(),
        "__transactions:commit:00000000000000000000000000000001".into(),
    ]);
    let tx = GenericTransactionsDecorator::new(
        store,
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    );
    assert!(
        tx.is_err(),
        "a failed replay must not be marked applied and hidden"
    );
}
