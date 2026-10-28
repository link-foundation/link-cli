//! Explicit-address allocation is a public transactional write.
use link_cli::{
    CommitMode, DoubletsStorage, FileTransitionLog, GenericTransactionsDecorator, LinkStorage,
    LinksStorage, LogRetentionPolicy, NamedTypesDecorator, TransactionsDecorator,
    VersionControlDecorator, DEFAULT_BRANCH_NAME,
};
use std::path::Path;

fn open(
    path: &Path,
    log: &Path,
) -> GenericTransactionsDecorator<u32, LinkStorage, FileTransitionLog> {
    GenericTransactionsDecorator::new(
        LinkStorage::new(path, false).unwrap(),
        FileTransitionLog::open(log).unwrap(),
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap()
}

#[test]
fn map_explicit_address_rolls_back_without_leaving_an_open_auto_transaction() {
    let directory = tempfile::tempdir().unwrap();
    let mut tx = open(
        &directory.path().join("data"),
        &directory.path().join("log"),
    );
    tx.begin_transaction().unwrap();
    assert_eq!(tx.ensure_created(8).unwrap(), 8);
    assert_eq!(tx.inner().links_count(), 1);
    tx.rollback().unwrap();
    assert!(!tx.exists(8), "explicit-address creation must roll back");
    let following = tx.create(11, 12).unwrap();
    assert!(tx.exists(following));
    assert_eq!(tx.applied_sequence(), tx.last_logged_sequence());
    tx.begin_transaction().unwrap();
    tx.rollback().unwrap();
}

#[test]
fn mapped_explicit_address_rolls_back_every_created_placeholder() {
    let directory = tempfile::tempdir().unwrap();
    let store = DoubletsStorage::<usize, _>::open_exclusive(directory.path().join("data")).unwrap();
    let mut tx = GenericTransactionsDecorator::new(
        store,
        FileTransitionLog::open(directory.path().join("log")).unwrap(),
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap();
    tx.begin_transaction().unwrap();
    tx.ensure_created(8).unwrap();
    assert_eq!(
        tx.inner().links_count(),
        8,
        "preserve mapped placeholder semantics"
    );
    tx.rollback().unwrap();
    assert_eq!(
        tx.inner().links_count(),
        0,
        "all placeholders must roll back"
    );
    tx.create(0, 0).unwrap();
    assert_eq!(tx.applied_sequence(), tx.last_logged_sequence());
}

#[test]
fn explicit_address_auto_commit_is_logged_durable_and_noops_do_not_open_transactions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("data");
    let log = directory.path().join("log");
    let mut tx = open(&path, &log);
    assert_eq!(tx.ensure_created(0).unwrap(), 0);
    assert!(tx.log().is_empty());
    tx.begin_transaction().unwrap();
    tx.rollback().unwrap();
    tx.ensure_created(8).unwrap();
    assert!(!tx.log().is_empty(), "auto allocation must be journaled");
    assert_eq!(tx.applied_sequence(), tx.last_logged_sequence());
    let sequence = tx.last_logged_sequence();
    tx.ensure_created(8).unwrap();
    assert_eq!(tx.last_logged_sequence(), sequence);
    tx.begin_transaction().unwrap();
    tx.rollback().unwrap();
    drop(tx);
    let tx = open(&path, &log);
    assert!(
        tx.exists(8),
        "auto allocation must survive reopening without save"
    );
    assert_eq!(tx.inner().links_count(), 1);
}

#[test]
fn uncommitted_explicit_address_is_undone_after_persisting_pending_data() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("data");
    let log = directory.path().join("log");
    let mut tx = open(&path, &log);
    tx.begin_transaction().unwrap();
    tx.ensure_created(8).unwrap();
    tx.flush().unwrap();
    drop(tx);
    let mut tx = open(&path, &log);
    assert!(!tx.exists(8));
    tx.reload().unwrap();
    assert!(tx.inner().all_links().is_empty());
    tx.create(11, 12).unwrap();
    assert_eq!(tx.applied_sequence(), tx.last_logged_sequence());
}

#[test]
fn mapped_uncommitted_placeholders_are_undone_after_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let open = || {
        GenericTransactionsDecorator::new(
            DoubletsStorage::<usize, _>::open_exclusive(directory.path().join("data")).unwrap(),
            FileTransitionLog::open(directory.path().join("log")).unwrap(),
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap()
    };
    let mut tx = open();
    tx.begin_transaction().unwrap();
    tx.ensure_created(8).unwrap();
    tx.flush().unwrap();
    drop(tx);
    let mut tx = open();
    assert_eq!(tx.inner().links_count(), 0);
    tx.reload().unwrap();
    assert_eq!(tx.inner().links_count(), 0);
}

fn versioned(directory: &Path) -> VersionControlDecorator {
    let tx = TransactionsDecorator::new(
        NamedTypesDecorator::new(directory.join("data"), false).unwrap(),
        NamedTypesDecorator::new(directory.join("log"), false).unwrap(),
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap();
    VersionControlDecorator::new(
        tx,
        NamedTypesDecorator::new(directory.join("versions"), false).unwrap(),
        false,
    )
    .unwrap()
}

#[test]
fn explicit_address_allocation_belongs_to_its_version_branch() {
    let directory = tempfile::tempdir().unwrap();
    let mut vc = versioned(directory.path());
    vc.branch("feature", None).unwrap();
    vc.switch_branch("feature").unwrap();
    assert_eq!(vc.ensure_created(8), 8);
    assert!(
        vc.current_sequence() > 0,
        "branch head must include allocation"
    );
    let head = vc.current_sequence();
    vc.switch_branch(DEFAULT_BRANCH_NAME).unwrap();
    assert!(!vc.exists(8), "feature allocation must not leak into main");
    vc.switch_branch("feature").unwrap();
    assert!(vc.exists(8));
    vc.checkout(0).unwrap();
    assert!(!vc.exists(8));
    vc.checkout(head).unwrap();
    assert!(vc.exists(8));
    vc.save().unwrap();
    drop(vc);
    let vc = versioned(directory.path());
    assert_eq!(vc.current_branch(), "feature");
    assert!(vc.exists(8));
}

#[test]
fn explicit_version_transaction_attributes_only_committed_allocation() {
    let directory = tempfile::tempdir().unwrap();
    let mut vc = versioned(directory.path());
    vc.begin_transaction().unwrap();
    vc.ensure_created(8);
    vc.rollback().unwrap();
    assert!(!vc.exists(8));
    assert_eq!(vc.current_sequence(), 0);
    vc.begin_transaction().unwrap();
    vc.ensure_created(8);
    assert_eq!(vc.current_sequence(), 0);
    vc.commit().unwrap();
    let head = vc.current_sequence();
    assert!(head > 0);
    vc.ensure_created(8);
    vc.ensure_created(0);
    assert_eq!(vc.current_sequence(), head);
    vc.checkout(0).unwrap();
    assert!(!vc.exists(8));
    vc.checkout(head).unwrap();
    assert!(vc.exists(8));
}

fn rejects_invalid_bridge_addresses<S: link_cli::NamedTypeLinks>(store: &mut S) {
    for address in [0, u32::MAX] {
        assert!(store.try_ensure_created(address).is_err());
    }
    assert!(store.all_links().is_empty());
}

#[test]
fn fallible_transaction_and_version_bridges_validate_addresses_before_allocation() {
    let directory = tempfile::tempdir().unwrap();
    let mut vc = versioned(directory.path());
    rejects_invalid_bridge_addresses(&mut vc);
    rejects_invalid_bridge_addresses(vc.transactions_mut());
    assert_eq!(vc.transactions().last_logged_sequence(), 0);
}

fn verify_fallible_bridge<S: link_cli::NamedTypeLinks>(
    mut store: S,
    directory: &Path,
    operation: &str,
) {
    std::fs::remove_file(directory.join("log")).unwrap();
    std::fs::create_dir(directory.join("log")).unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if operation == "create" {
            store.try_create(0, 0)
        } else {
            store.try_ensure_created(8)
        }
    }));
    assert!(
        matches!(outcome, Ok(Err(_))),
        "fallible {operation} must return an error"
    );
}

fn verify_named_bridge(versioned_store: bool, operation: &str) {
    let directory = tempfile::tempdir().unwrap();
    if versioned_store {
        verify_fallible_bridge(versioned(directory.path()), directory.path(), operation);
    } else {
        let store = TransactionsDecorator::new(
            NamedTypesDecorator::new(directory.path().join("data"), false).unwrap(),
            NamedTypesDecorator::new(directory.path().join("log"), false).unwrap(),
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap();
        verify_fallible_bridge(store, directory.path(), operation);
    }
}

#[test]
fn fallible_transaction_create_returns_journal_error() {
    verify_named_bridge(false, "create");
}
#[test]
fn fallible_transaction_ensure_returns_journal_error() {
    verify_named_bridge(false, "ensure");
}
#[test]
fn fallible_version_create_returns_journal_error() {
    verify_named_bridge(true, "create");
}
#[test]
fn fallible_version_ensure_returns_journal_error() {
    verify_named_bridge(true, "ensure");
}
