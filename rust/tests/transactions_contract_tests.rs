//! Public contracts of the transactions decorator that other suites reach only
//! indirectly: accessors, adapters that report nothing, and the file log's edges.
use link_cli::{
    engine::MemoryLog, CommitMode, FileTransitionLog, GenericLink, GenericTransactionsDecorator,
    Link, LinkError, LinkStorage, LinksStorage, LinksStorageRef, LogRetentionPolicy, RowUndoPolicy,
    TransitionKind, TransitionLogStore,
};
use std::cell::Cell;

#[path = "common/storage.rs"]
mod test_store;

#[derive(Clone, Copy, PartialEq)]
enum Reports {
    /// The adapter makes the change and calls no observer at all.
    Nothing,
    /// The adapter reports a pair with no address on either side.
    EmptyPair,
}

struct Store {
    rows: LinkStorage,
    reports: Reports,
    fail_delete: bool,
    /// The next update makes the following enumeration return a duplicate row.
    corrupt_after_update: bool,
    corrupt_next: Cell<bool>,
}

impl Store {
    fn new(reports: Reports) -> Self {
        Self {
            rows: LinkStorage::in_memory(false),
            reports,
            fail_delete: false,
            corrupt_after_update: false,
            corrupt_next: Cell::new(false),
        }
    }
    fn report(&self, observer: &mut dyn FnMut(Link, Link)) {
        if self.reports == Reports::EmptyPair {
            observer(GenericLink::null(), GenericLink::null());
        }
    }
}

impl LinksStorage<u32> for Store {
    fn create_link(&mut self, source: u32, target: u32) -> Result<u32, LinkError> {
        Ok(self.rows.create(source, target))
    }
    fn get_link(&self, id: u32) -> Option<Link> {
        self.rows.get(id).copied()
    }
    fn ensure_link_created(&mut self, id: u32) -> Result<u32, LinkError> {
        LinksStorage::ensure_link_created(&mut self.rows, id)
    }
    fn all_links(&self) -> Vec<Link> {
        let mut rows: Vec<Link> = self.rows.all().into_iter().copied().collect();
        if self.corrupt_next.replace(false) {
            rows.extend(rows.first().copied());
        }
        rows
    }
    fn update_link(&mut self, id: u32, source: u32, target: u32) -> Result<Link, LinkError> {
        LinksStorage::update_link(&mut self.rows, id, source, target)
    }
    fn delete_link(&mut self, id: u32) -> Result<Link, LinkError> {
        LinksStorage::delete_link(&mut self.rows, id)
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        Ok(())
    }
    fn update_link_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: &mut dyn FnMut(Link, Link),
    ) -> Result<Link, LinkError> {
        self.report(observer);
        self.corrupt_next
            .set(std::mem::take(&mut self.corrupt_after_update));
        LinksStorage::update_link(&mut self.rows, id, source, target)
    }
    fn delete_link_observed(
        &mut self,
        id: u32,
        observer: &mut dyn FnMut(Link, Link),
    ) -> Result<Link, LinkError> {
        if self.fail_delete {
            return Err(LinkError::StorageError("injected delete".into()));
        }
        self.report(observer);
        LinksStorage::delete_link(&mut self.rows, id)
    }
}

impl LinksStorageRef<u32> for Store {
    fn get_link_ref(&self, id: u32) -> Option<&Link> {
        self.rows.get(id)
    }
    fn all_link_refs(&self) -> Vec<&Link> {
        self.rows.all()
    }
}

type Tx = GenericTransactionsDecorator<u32, Store, MemoryLog>;

fn open(reports: Reports) -> Tx {
    GenericTransactionsDecorator::new(
        Store::new(reports),
        MemoryLog::default(),
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap()
}

fn kinds(tx: &Tx) -> Vec<TransitionKind> {
    tx.log().iter().map(|entry| entry.kind).collect()
}

#[test]
fn settings_are_readable_and_replaceable() {
    let mut tx = open(Reports::Nothing);
    assert!(matches!(
        tx.retention_policy(),
        LogRetentionPolicy::Infinite
    ));
    tx.set_retention_policy(LogRetentionPolicy::Sized { max_transitions: 3 });
    assert!(matches!(
        tx.retention_policy(),
        LogRetentionPolicy::Sized { max_transitions: 3 }
    ));
    assert_eq!(tx.commit_mode(), CommitMode::Sync);
    tx.set_commit_mode(CommitMode::Async);
    assert_eq!(tx.commit_mode(), CommitMode::Async);
}

#[test]
fn reload_is_refused_during_a_transaction_and_keeps_data_afterwards() {
    let mut tx = open(Reports::Nothing);
    let id = tx.create(1, 2).unwrap();
    assert!(!tx.has_external_changes().unwrap());
    tx.begin_transaction().unwrap();
    let refused = tx.reload().unwrap_err();
    assert!(refused.to_string().contains("Cannot reload"), "{refused}");
    tx.rollback().unwrap();
    tx.reload().unwrap();
    assert_eq!(tx.get(id).map(|row| (row.source, row.target)), Some((1, 2)));
}

#[test]
fn query_filters_the_wrapped_rows() {
    let mut tx = open(Reports::Nothing);
    let first = tx.create(1, 2).unwrap();
    tx.create(3, 4).unwrap();
    let found = tx.query(None, Some(1), None);
    assert_eq!(
        found.iter().map(|row| row.index).collect::<Vec<_>>(),
        [first]
    );
}

/// An unreported change must still be journaled, or rollback could not undo it.
#[test]
fn unreported_update_and_delete_are_journaled_and_undone() {
    let mut tx = open(Reports::Nothing);
    let id = tx.create(1, 2).unwrap();

    tx.begin_transaction().unwrap();
    tx.update(id, 5, 6).unwrap();
    assert_eq!(kinds(&tx).last(), Some(&TransitionKind::Update));
    tx.rollback().unwrap();
    assert_eq!(tx.get(id).map(|row| (row.source, row.target)), Some((1, 2)));

    tx.begin_transaction().unwrap();
    tx.delete(id).unwrap();
    assert_eq!(kinds(&tx).last(), Some(&TransitionKind::Delete));
    tx.rollback().unwrap();
    assert_eq!(tx.get(id).map(|row| (row.source, row.target)), Some((1, 2)));
}

/// Nothing has changed, so nothing is left to undo and the store stays usable.
#[test]
fn failed_unreported_delete_releases_the_automatic_owner() {
    let mut tx = open(Reports::Nothing);
    let id = tx.create(1, 2).unwrap();
    tx.inner_mut().fail_delete = true;
    let journaled = tx.journal().len();
    let error = tx.delete(id).unwrap_err();
    assert!(error.to_string().contains("injected delete"), "{error}");
    assert_eq!(tx.journal().len(), journaled);
    assert!(tx.get(id).is_some());
    tx.inner_mut().fail_delete = false;
    tx.delete(id).unwrap();
    assert!(tx.get(id).is_none());
}

/// A pair with no address describes nothing and cannot name a row to undo.
#[test]
fn observations_without_an_address_are_ignored() {
    let mut tx = open(Reports::EmptyPair);
    let id = tx.create(1, 2).unwrap();
    tx.update(id, 5, 6).unwrap();
    assert_eq!(kinds(&tx), [TransitionKind::Create, TransitionKind::Update]);
}

#[test]
fn image_undo_predicts_deletes_before_later_writes() {
    let mut tx = open(Reports::Nothing)
        .with_row_undo(RowUndoPolicy::WriteAheadImage)
        .unwrap();
    let kept = tx.create(10, 20).unwrap();
    let removed = tx.create(30, 40).unwrap();

    tx.begin_transaction().unwrap();
    tx.delete(removed).unwrap();
    // The second write compares its result with the first write's prediction.
    let journaled = tx.journal().len();
    tx.update(kept, 7, 8).unwrap();
    assert_eq!(tx.journal().len(), journaled + 1, "no correction record");
    tx.rollback().unwrap();
    assert_eq!(
        tx.get(kept).map(|row| (row.source, row.target)),
        Some((10, 20))
    );
    assert_eq!(
        tx.get(removed).map(|row| (row.source, row.target)),
        Some((30, 40))
    );
}

#[test]
fn explicit_image_owner_must_abort_when_a_write_reveals_invalid_rows() {
    let mut tx = open(Reports::Nothing)
        .with_row_undo(RowUndoPolicy::WriteAheadImage)
        .unwrap();
    let id = tx.create(1, 2).unwrap();
    tx.begin_transaction().unwrap();
    tx.inner_mut().corrupt_after_update = true;
    let error = tx.update(id, 5, 6).unwrap_err();
    assert!(
        error.to_string().contains("invalid row enumeration"),
        "{error}"
    );
    assert!(tx.commit().is_err(), "the owner is fenced");
    tx.rollback().unwrap();
    assert_eq!(tx.get(id).map(|row| (row.source, row.target)), Some((1, 2)));
}

#[test]
fn automatic_image_owner_undoes_a_write_that_reveals_invalid_rows() {
    let mut tx = open(Reports::Nothing)
        .with_row_undo(RowUndoPolicy::WriteAheadImage)
        .unwrap();
    let id = tx.create(1, 2).unwrap();
    tx.inner_mut().corrupt_after_update = true;
    let error = tx.update(id, 5, 6).unwrap_err();
    assert!(
        error.to_string().contains("invalid row enumeration"),
        "{error}"
    );
    assert_eq!(tx.get(id).map(|row| (row.source, row.target)), Some((1, 2)));
    tx.begin_transaction().unwrap();
    tx.commit().unwrap();
}

#[test]
fn file_log_creates_parents_and_reports_its_settings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("a/b/journal.log");
    let mut log = FileTransitionLog::open(&path).unwrap();
    assert_eq!(log.path(), path);
    assert!(log.sync_on_append());
    assert!(log.supports_durable_flush());
    let error = log.append_log_entry("two\nlines").unwrap_err();
    assert!(error.to_string().contains("line breaks"), "{error}");
    let error = log.append_log_entry("two\rlines").unwrap_err();
    assert!(error.to_string().contains("line breaks"), "{error}");
}

#[test]
fn file_log_accepts_a_path_without_a_directory() {
    let name = format!("transactions-contract-{}.log", std::process::id());
    let result = FileTransitionLog::open(&name).map(|log| log.path().to_path_buf());
    let _ = std::fs::remove_file(&name);
    assert_eq!(result.unwrap(), std::path::Path::new(&name));
}

/// A crash during the very first append leaves one line with no terminator.
#[test]
fn file_log_drops_a_tail_with_no_complete_line() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.log");
    std::fs::write(&path, b"fragment").unwrap();
    let mut log = FileTransitionLog::open(&path).unwrap();
    assert!(log.read_log_entries().unwrap().is_empty());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
}

#[test]
fn named_decorator_log_is_durable_only_when_it_has_a_path() {
    let (_files, named) = test_store::named().unwrap();
    assert_eq!(
        named.supports_durable_flush(),
        !named.links().database_path().as_os_str().is_empty()
    );
}

/// `LinkStorage::update` merges duplicate doublets, which `create` allows. Undo
/// restores recorded rows, so it must not merge the row it is putting back.
#[test]
fn rollback_restores_duplicate_rows_exactly() {
    let rows = |tx: &GenericTransactionsDecorator<u32, LinkStorage, MemoryLog>| {
        tx.all().into_iter().copied().collect::<Vec<_>>()
    };
    let mut tx = GenericTransactionsDecorator::new(
        LinkStorage::in_memory(false),
        MemoryLog::default(),
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap();
    tx.create(10, 20).unwrap();
    tx.create(10, 20).unwrap();
    let expected = rows(&tx);
    assert_eq!(expected.len(), 2);

    tx.begin_transaction().unwrap();
    tx.update(2, 50, 60).unwrap();
    tx.rollback().unwrap();
    assert_eq!(rows(&tx), expected, "undone update");

    tx.begin_transaction().unwrap();
    tx.delete(2).unwrap();
    tx.rollback().unwrap();
    assert_eq!(rows(&tx), expected, "undone delete");

    let mut tx = tx.with_row_undo(RowUndoPolicy::WriteAheadImage).unwrap();
    tx.begin_transaction().unwrap();
    tx.update(2, 50, 60).unwrap();
    tx.rollback().unwrap();
    assert_eq!(rows(&tx), expected, "restored image");
}
