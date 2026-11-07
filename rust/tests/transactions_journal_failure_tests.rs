//! A journal failure must retain every undo record of an observed cascade.
use link_cli::{
    CommitMode, DoubletsStorage, FileTransitionLog, GenericTransactionsDecorator, LinkError,
    LinkStorage, LinksStorage, LogRetentionPolicy, TransitionLogStore,
};

struct FaultLog {
    inner: FileTransitionLog,
    remaining: usize,
    after: bool,
    fail_rollback: bool,
}
impl TransitionLogStore for FaultLog {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        if entry.starts_with("__transactions:rollback:") && self.fail_rollback {
            self.fail_rollback = false;
            return Err(LinkError::StorageError("injected rollback marker".into()));
        }
        let fail = if entry.starts_with("__transactions:transition:") && self.remaining > 0 {
            self.remaining -= 1;
            self.remaining == 0
        } else {
            false
        };
        if fail && !self.after {
            return Err(LinkError::StorageError(
                "injected journal before append".into(),
            ));
        }
        self.inner.append_log_entry(entry)?;
        if fail {
            return Err(LinkError::StorageError(
                "injected journal after append".into(),
            ));
        }
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.inner.read_log_entries()
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.inner.flush_log()
    }
}
fn verify(explicit: bool, after: bool, fail_at: usize, reject_commit: bool, cleanup_failure: bool) {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let log = directory.path().join("log");
    let open = || {
        GenericTransactionsDecorator::new(
            DoubletsStorage::<u32, _>::open(&data)
                .unwrap()
                .with_automatic_uniqueness_and_usages_resolution(),
            FaultLog {
                inner: FileTransitionLog::open(&log).unwrap(),
                remaining: 0,
                after,
                fail_rollback: false,
            },
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap()
    };
    let mut tx = open();
    let a = tx.create(0, 0).unwrap();
    let b = tx.create(a, a).unwrap();
    let c = tx.create(b, b).unwrap();
    let baseline = tx.inner().all_links();
    if explicit {
        tx.begin_transaction().unwrap();
    }
    tx.log_store_mut().remaining = fail_at;
    tx.log_store_mut().fail_rollback = cleanup_failure;
    let error = tx.delete(b).unwrap_err();
    assert!(error.to_string().contains(if cleanup_failure {
        "injected rollback marker"
    } else {
        "injected journal"
    }));
    if reject_commit {
        assert!(
            tx.commit().is_err(),
            "incomplete journal must not acknowledge commit"
        );
        assert!(
            tx.create(31, 32).is_err(),
            "failed explicit transaction must reject further writes"
        );
    }
    if explicit || cleanup_failure {
        tx.rollback().unwrap();
    }
    assert_eq!(
        tx.inner().all_links(),
        baseline,
        "explicit={explicit} after={after} fail_at={fail_at}"
    );
    assert!(tx.exists(c));
    tx.begin_transaction()
        .expect("automatic rollback must release admission");
    tx.rollback().unwrap();
    let following = tx.create(c, c).unwrap();
    let expected = tx.inner().all_links();
    drop(tx);
    let reopened = open();
    assert_eq!(reopened.inner().all_links(), expected);
    assert!(reopened.exists(following));
}
macro_rules! case {
    ($name:ident, $explicit:expr, $after:expr, $at:expr, $commit:expr) => {
        #[test]
        fn $name() {
            verify($explicit, $after, $at, $commit, false);
        }
    };
}
case!(automatic_first_append_before, false, false, 1, false);
case!(automatic_first_append_after, false, true, 1, false);
case!(automatic_second_append_before, false, false, 2, false);
case!(automatic_second_append_after, false, true, 2, false);
case!(explicit_first_append_before, true, false, 1, false);
case!(explicit_first_append_after, true, true, 1, false);
case!(explicit_second_append_before, true, false, 2, false);
case!(explicit_second_append_after, true, true, 2, false);
case!(commit_rejected_after_first_before, true, false, 1, true);
case!(commit_rejected_after_first_after, true, true, 1, true);
case!(commit_rejected_after_second_before, true, false, 2, true);
case!(commit_rejected_after_second_after, true, true, 2, true);

#[test]
fn automatic_cleanup_marker_failure_stays_pending_before_append() {
    verify(false, false, 1, true, true);
}

#[test]
fn automatic_cleanup_marker_failure_stays_pending_after_append() {
    verify(false, true, 1, true, true);
}

fn verify_single_write(operation: &str, after: bool) {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let log = directory.path().join("log");
    let open = || {
        GenericTransactionsDecorator::new(
            LinkStorage::new(&data, false).unwrap(),
            FaultLog {
                inner: FileTransitionLog::open(&log).unwrap(),
                remaining: 0,
                after,
                fail_rollback: false,
            },
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap()
    };
    let mut tx = open();
    let id = tx.create(11, 12).unwrap();
    let baseline = tx.inner().all_links();
    tx.log_store_mut().remaining = 1;
    let outcome = match operation {
        "create" => tx.create(21, 22),
        "update" => tx.update(id, 21, 22).map(|link| link.index),
        "ensure" => tx.ensure_created(3),
        _ => unreachable!(),
    };
    assert!(outcome
        .unwrap_err()
        .to_string()
        .contains("injected journal"));
    assert_eq!(tx.inner().all_links(), baseline);
    tx.begin_transaction().unwrap();
    tx.rollback().unwrap();
    tx.create(31, 32).unwrap();
    let expected = tx.inner().all_links();
    drop(tx);
    assert_eq!(open().inner().all_links(), expected);
}

macro_rules! single_case {
    ($name:ident, $operation:expr, $after:expr) => {
        #[test]
        fn $name() {
            verify_single_write($operation, $after);
        }
    };
}
single_case!(create_append_before, "create", false);
single_case!(create_append_after, "create", true);
single_case!(update_append_before, "update", false);
single_case!(update_append_after, "update", true);
single_case!(ensure_append_before, "ensure", false);
single_case!(ensure_append_after, "ensure", true);
