//! A failed rollback must fence commit and writes until cleanup succeeds.
use std::{cell::Cell, rc::Rc};

use link_cli::{
    CommitMode, FileTransitionLog, GenericTransactionsDecorator, Link, LinkError, LinkStorage,
    LinksStorage, LogRetentionPolicy, TransitionLogStore,
};

#[derive(Clone, Default)]
struct Fault(Rc<Cell<Option<&'static str>>>);
impl Fault {
    fn check(&self, point: &str) -> Result<(), LinkError> {
        if self.0.get() == Some(point) {
            self.0.set(None);
            return Err(LinkError::StorageError(format!("injected {point}")));
        }
        Ok(())
    }
}
struct Store {
    inner: LinkStorage,
    fault: Fault,
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
        self.fault.check("undo-before")?;
        let result = LinksStorage::update_link(&mut self.inner, id, source, target)?;
        self.fault.check("undo-after")?;
        Ok(result)
    }
    fn delete_link(&mut self, id: u32) -> Result<Link, LinkError> {
        LinksStorage::delete_link(&mut self.inner, id)
    }
    fn all_links(&self) -> Vec<Link> {
        self.inner.all().into_iter().copied().collect()
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        self.fault.check("data-before")?;
        LinksStorage::flush(&mut self.inner)?;
        self.fault.check("data-after")
    }
}
struct Log {
    inner: FileTransitionLog,
    fault: Fault,
}
impl TransitionLogStore for Log {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        let rollback = entry.starts_with("__transactions:rollback:");
        if rollback {
            self.fault.check("marker-before")?;
        }
        self.inner.append_log_entry(entry)?;
        if rollback {
            self.fault.check("marker-after")?;
        }
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.inner.read_log_entries()
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.fault.check("log-before")?;
        self.inner.flush_log()?;
        self.fault.check("log-after")
    }
}
fn verify(point: &'static str, commit: bool) {
    let directory = tempfile::tempdir().unwrap();
    let fault = Fault::default();
    let open = || {
        GenericTransactionsDecorator::new(
            Store {
                inner: LinkStorage::new(directory.path().join("data"), false).unwrap(),
                fault: fault.clone(),
            },
            Log {
                inner: FileTransitionLog::open(directory.path().join("log")).unwrap(),
                fault: fault.clone(),
            },
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap()
    };
    let mut tx = open();
    let a = tx.create(11, 12).unwrap();
    let b = tx.create(21, 22).unwrap();
    let baseline = tx.inner().all_links();
    tx.begin_transaction().unwrap();
    tx.update(a, 31, 32).unwrap();
    tx.update(b, 41, 42).unwrap();
    fault.0.set(Some(point));
    assert!(tx.rollback().unwrap_err().to_string().contains(point));
    assert!(tx.begin_transaction().is_err());
    if commit {
        assert!(tx.commit().is_err(), "failed rollback must reject commit");
    } else {
        assert!(
            tx.create(51, 52).is_err(),
            "failed rollback must reject further writes"
        );
    }
    tx.rollback().unwrap();
    assert_eq!(tx.inner().all_links(), baseline);
    tx.begin_transaction().unwrap();
    tx.rollback().unwrap();
    let following = tx.create(51, 52).unwrap();
    let expected = tx.inner().all_links();
    drop(tx);
    let reopened = open();
    assert_eq!(reopened.inner().all_links(), expected);
    assert!(reopened.exists(following));
}
macro_rules! case {
    ($name:ident, $point:expr, $commit:expr) => {
        #[test]
        fn $name() {
            verify($point, $commit);
        }
    };
}
case!(undo_before_rejects_commit, "undo-before", true);
case!(undo_after_rejects_commit, "undo-after", true);
case!(data_before_rejects_commit, "data-before", true);
case!(data_after_rejects_commit, "data-after", true);
case!(marker_before_rejects_commit, "marker-before", true);
case!(marker_after_rejects_commit, "marker-after", true);
case!(log_before_rejects_commit, "log-before", true);
case!(log_after_rejects_commit, "log-after", true);
case!(undo_before_rejects_write, "undo-before", false);
case!(undo_after_rejects_write, "undo-after", false);
case!(data_before_rejects_write, "data-before", false);
case!(data_after_rejects_write, "data-after", false);
case!(marker_before_rejects_write, "marker-before", false);
case!(marker_after_rejects_write, "marker-after", false);
case!(log_before_rejects_write, "log-before", false);
case!(log_after_rejects_write, "log-after", false);
