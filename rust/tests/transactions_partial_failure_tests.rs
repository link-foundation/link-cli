//! Observed partial writes must remain reversible when a storage adapter fails.
use link_cli::{
    CommitMode, FileTransitionLog, GenericTransactionsDecorator, Link, LinkError, LinkStorage,
    LinksStorage, LogRetentionPolicy,
};

#[derive(Clone, Copy, Debug)]
enum Operation {
    Update,
    Delete,
    CreateThenUpdate,
}
struct Store {
    inner: LinkStorage,
    fail: bool,
    cascade: bool,
}
impl LinksStorage<u32> for Store {
    fn create_link(&mut self, source: u32, target: u32) -> Result<u32, LinkError> {
        Ok(self.inner.create(source, target))
    }
    fn ensure_link_created(&mut self, id: u32) -> Result<u32, LinkError> {
        Ok(self.inner.ensure_created(id))
    }
    fn get_link(&self, id: u32) -> Option<Link> {
        self.inner.get(id).copied()
    }
    fn update_link(&mut self, id: u32, source: u32, target: u32) -> Result<Link, LinkError> {
        LinksStorage::update_link(&mut self.inner, id, source, target)
    }
    fn delete_link(&mut self, id: u32) -> Result<Link, LinkError> {
        LinksStorage::delete_link(&mut self.inner, id)
    }
    fn all_links(&self) -> Vec<Link> {
        self.inner.all().into_iter().copied().collect()
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        LinksStorage::flush(&mut self.inner)
    }
    fn update_link_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: &mut dyn FnMut(Link, Link),
    ) -> Result<Link, LinkError> {
        let before = self.update_link(id, source, target)?;
        observer(before, self.get_link(id).unwrap());
        if self.cascade {
            let added = self.create_link(31, 32)?;
            let created = self.get_link(added).unwrap();
            observer(Link::null(), created);
            self.update_link(added, 41, 42)?;
            observer(created, self.get_link(added).unwrap());
        }
        if self.fail {
            Err(LinkError::StorageError("injected after update".into()))
        } else {
            Ok(before)
        }
    }
    fn delete_link_observed(
        &mut self,
        id: u32,
        observer: &mut dyn FnMut(Link, Link),
    ) -> Result<Link, LinkError> {
        let before = self.delete_link(id)?;
        observer(before, Link::null());
        if self.fail {
            Err(LinkError::StorageError("injected after delete".into()))
        } else {
            Ok(before)
        }
    }
}
fn verify(operation: Operation, explicit: bool, fail: bool) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let log = dir.path().join("log");
    let mut tx = GenericTransactionsDecorator::new(
        Store {
            inner: LinkStorage::new(&data, false).unwrap(),
            fail: false,
            cascade: false,
        },
        FileTransitionLog::open(&log).unwrap(),
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap();
    let id = tx.create(11, 12).unwrap();
    let baseline = tx.inner().all_links();
    if explicit {
        tx.begin_transaction().unwrap();
    }
    tx.inner_mut().fail = fail;
    tx.inner_mut().cascade = matches!(operation, Operation::CreateThenUpdate);
    let outcome = match operation {
        Operation::Delete => tx.delete(id),
        _ => tx.update(id, 21, 22),
    };
    if fail {
        let error = outcome.unwrap_err();
        assert!(matches!(error, LinkError::StorageError(_)));
        assert!(error.to_string().contains("injected after"));
    } else {
        outcome.unwrap();
    }
    tx.inner_mut().fail = false;
    tx.inner_mut().cascade = false;
    if explicit {
        assert!(tx.begin_transaction().is_err());
        tx.rollback().unwrap();
    }
    assert_eq!(
        tx.inner().all_links(),
        baseline,
        "{operation:?}, explicit={explicit}, fail={fail}"
    );
    tx.begin_transaction()
        .expect("automatic failure must release admission");
    tx.rollback().unwrap();
    let following = tx.create(51, 52).unwrap();
    assert!(tx.exists(following));
    let expected = tx.inner().all_links();
    drop(tx);
    let reopened = GenericTransactionsDecorator::new(
        Store {
            inner: LinkStorage::new(&data, false).unwrap(),
            fail: false,
            cascade: false,
        },
        FileTransitionLog::open(&log).unwrap(),
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap();
    assert_eq!(reopened.inner().all_links(), expected);
}
#[test]
fn failed_observed_update_undoes_automatic_partial_write() {
    verify(Operation::Update, false, true);
}
#[test]
fn failed_observed_update_remains_in_explicit_rollback() {
    verify(Operation::Update, true, true);
}
#[test]
fn failed_observed_delete_undoes_automatic_partial_write() {
    verify(Operation::Delete, false, true);
}
#[test]
fn failed_observed_delete_remains_in_explicit_rollback() {
    verify(Operation::Delete, true, true);
}
#[test]
fn repeated_callbacks_keep_original_absent_state() {
    verify(Operation::CreateThenUpdate, true, false);
}
#[test]
fn failed_cascade_keeps_original_absent_state() {
    verify(Operation::CreateThenUpdate, false, true);
}
