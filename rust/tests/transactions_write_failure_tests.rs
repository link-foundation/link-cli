//! Failed automatic operations must release admission and undo partial writes.
use link_cli::{
    CommitMode, FileTransitionLog, GenericTransactionsDecorator, Link, LinkError, LinkStorage,
    LinksStorage, LogRetentionPolicy,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    Create,
    Update,
    EnsureBefore,
    EnsureAfter,
}
struct Store {
    inner: LinkStorage,
    fault: Option<Fault>,
}
impl LinksStorage<u32> for Store {
    fn create_link(&mut self, source: u32, target: u32) -> Result<u32, LinkError> {
        if self.fault == Some(Fault::Create) {
            return Err(LinkError::StorageError("injected create".into()));
        }
        Ok(self.inner.create(source, target))
    }
    fn ensure_link_created(&mut self, id: u32) -> Result<u32, LinkError> {
        if self.fault == Some(Fault::EnsureBefore) {
            return Err(LinkError::StorageError("injected ensure".into()));
        }
        let created = self.inner.ensure_created(id);
        if self.fault == Some(Fault::EnsureAfter) {
            return Err(LinkError::StorageError("injected partial ensure".into()));
        }
        Ok(created)
    }
    fn get_link(&self, id: u32) -> Option<Link> {
        self.inner.get(id).copied()
    }
    fn update_link(&mut self, id: u32, source: u32, target: u32) -> Result<Link, LinkError> {
        if self.fault == Some(Fault::Update) {
            return Err(LinkError::StorageError("injected update".into()));
        }
        LinksStorage::update_link(&mut self.inner, id, source, target)
    }
    fn delete_link(&mut self, id: u32) -> Result<Link, LinkError> {
        LinksStorage::delete_link(&mut self.inner, id)
    }
    fn all_links(&self) -> Vec<Link> {
        self.inner.all().into_iter().copied().collect()
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        Ok(())
    }
}

fn verify(fault: Fault, composite: bool) {
    for explicit in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut tx = GenericTransactionsDecorator::new(
            Store {
                inner: LinkStorage::in_memory(false),
                fault: None,
            },
            FileTransitionLog::open(directory.path().join("log")).unwrap(),
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap();
        if explicit {
            tx.begin_transaction().unwrap();
            tx.create(11, 12).unwrap();
        }
        tx.inner_mut().fault = Some(fault);
        let error = if composite {
            tx.create_and_update(21, 22)
        } else if matches!(fault, Fault::EnsureBefore | Fault::EnsureAfter) {
            tx.ensure_created(8)
        } else {
            tx.create(21, 22)
        }
        .unwrap_err();
        assert!(
            matches!(error, LinkError::StorageError(_)),
            "preserve the storage error"
        );
        tx.inner_mut().fault = None;
        if explicit {
            assert!(
                tx.begin_transaction().is_err(),
                "caller still owns its transaction"
            );
            tx.rollback().unwrap();
        } else {
            assert!(
                tx.inner().all_links().is_empty(),
                "failed automatic work must be undone"
            );
            tx.begin_transaction()
                .expect("failed automatic work must release admission");
            tx.rollback().unwrap();
        }
        assert!(tx.inner().all_links().is_empty());
        let following = tx.create(31, 32).unwrap();
        assert!(tx.exists(following));
        assert_eq!(tx.applied_sequence(), tx.last_logged_sequence());
    }
}

#[test]
fn failed_create_releases_only_automatic_transaction() {
    verify(Fault::Create, false);
}
#[test]
fn failed_composite_create_releases_only_automatic_transaction() {
    verify(Fault::Create, true);
}
#[test]
fn failed_composite_update_undoes_automatic_create() {
    verify(Fault::Update, true);
}
#[test]
fn failed_ensure_releases_only_automatic_transaction() {
    verify(Fault::EnsureBefore, false);
}
#[test]
fn partial_ensure_remains_reversible() {
    verify(Fault::EnsureAfter, false);
}
