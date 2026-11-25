//! Issue #195: the journal is an append-only file whose size a retention
//! policy bounds, and the older links-backed journal is imported once.
use std::fs;
use std::path::{Path, PathBuf};

use link_cli::{
    import_links_journal, is_links_journal, make_legacy_transitions_filename,
    make_transitions_log_filename, open_transitions_journal, CommitMode, FileTransitionLog,
    GenericTransactionsDecorator, LinkStorage, LogRetentionPolicy, NamedTypesDecorator,
    TransitionLogStore,
};

type Tx = GenericTransactionsDecorator<u32, LinkStorage, FileTransitionLog>;

fn open(dir: &Path, policy: LogRetentionPolicy) -> Tx {
    let store = LinkStorage::new(dir.join("data"), false).unwrap();
    let log = FileTransitionLog::open(dir.join("journal.log")).unwrap();
    GenericTransactionsDecorator::new(store, log, policy, CommitMode::Sync, false).unwrap()
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn compaction_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".compact");
    PathBuf::from(name)
}

#[test]
fn retention_bounds_the_journal_file_and_survives_reopening() {
    for chunked in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let policy = || {
            if chunked {
                LogRetentionPolicy::Chunked {
                    chunk_size: 8,
                    archive_directory: dir.path().join("archive"),
                }
            } else {
                LogRetentionPolicy::Sized { max_transitions: 8 }
            }
        };
        let journal = dir.path().join("journal.log");
        let mut tx = open(dir.path(), policy());
        let mut largest = 0;
        for i in 1..=200u32 {
            tx.create(i, i).unwrap();
            largest = largest.max(lines(&journal).len());
            assert_eq!(tx.journal_entry_count(), lines(&journal).len());
        }
        // Each retained record needs at most three entries; the journal is
        // compacted once it outgrows twice that plus a fixed slack.
        assert!(
            largest <= 2 * (3 * 8 + 3) + 256 + 3,
            "chunked={chunked}: {largest}"
        );
        assert!(!compaction_path(&journal).exists());
        let sequence = tx.last_logged_sequence();
        let applied = tx.applied_sequence();
        let view = tx.log();
        drop(tx);

        let mut tx = open(dir.path(), policy());
        assert_eq!(tx.last_logged_sequence(), sequence, "chunked={chunked}");
        assert_eq!(tx.applied_sequence(), applied, "chunked={chunked}");
        let reopened = tx.log();
        assert_eq!(reopened[reopened.len() - view.len()..], view[..]);
        tx.create(1, 1).unwrap();
        assert_eq!(tx.last_logged_sequence(), sequence + 1);
    }
}

#[test]
fn infinite_retention_keeps_every_entry() {
    let dir = tempfile::tempdir().unwrap();
    let mut tx = open(dir.path(), LogRetentionPolicy::Infinite);
    for i in 1..=150u32 {
        tx.create(i, i).unwrap();
    }
    assert_eq!(tx.journal().len(), 150);
    assert_eq!(
        tx.journal_entry_count(),
        lines(&dir.path().join("journal.log")).len()
    );
    assert!(tx.journal_entry_count() >= 3 * 150);
}

#[test]
fn a_compacted_journal_keeps_rolled_back_owners_and_the_newest_decision() {
    let dir = tempfile::tempdir().unwrap();
    let journal = dir.path().join("journal.log");
    let mut tx = open(dir.path(), LogRetentionPolicy::Sized { max_transitions: 2 });
    for i in 1..=150u32 {
        tx.create(i, i).unwrap();
    }
    for n in [8, 9] {
        tx.begin_transaction().unwrap();
        tx.create(n, n).unwrap();
        tx.rollback().unwrap();
    }
    for i in 1..=60u32 {
        tx.create(i, i).unwrap();
    }
    let entries = lines(&journal);
    assert!(
        entries[0].starts_with("__transactions:sequence:"),
        "{entries:?}"
    );
    let decision = entries
        .iter()
        .rev()
        .find(|entry| entry.starts_with("__transactions:commit:"))
        .cloned()
        .unwrap();
    let sequence = tx.last_logged_sequence();
    drop(tx);
    let tx = open(dir.path(), LogRetentionPolicy::Sized { max_transitions: 2 });
    assert_eq!(tx.last_logged_sequence(), sequence);
    assert!(lines(&journal).contains(&decision));
}

#[test]
fn the_sequence_watermark_reserves_numbers_and_must_be_positive() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("journal.log"),
        "__transactions:sequence:41\n",
    )
    .unwrap();
    let mut tx = open(dir.path(), LogRetentionPolicy::Infinite);
    assert_eq!(tx.last_logged_sequence(), 41);
    tx.create(1, 1).unwrap();
    assert_eq!(tx.last_logged_sequence(), 42);
    drop(tx);

    for bad in ["0", "-3", "x", ""] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("journal.log"),
            format!("__transactions:sequence:{bad}\n"),
        )
        .unwrap();
        let store = LinkStorage::new(dir.path().join("data"), false).unwrap();
        let log = FileTransitionLog::open(dir.path().join("journal.log")).unwrap();
        let error = GenericTransactionsDecorator::new(
            store,
            log,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .err()
        .unwrap();
        assert!(
            error.to_string().contains("invalid sequence marker"),
            "{bad}: {error}"
        );
    }
}

#[test]
fn file_log_compaction_replaces_the_journal_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.log");
    let mut log = FileTransitionLog::open(&path).unwrap();
    assert!(log.supports_compaction());
    for entry in ["a", "b", "c"] {
        log.append_log_entry(entry).unwrap();
    }
    log.compact_log_entries(&["b".to_owned()]).unwrap();
    assert!(!compaction_path(&path).exists());
    log.append_log_entry("d").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "b\nd\n");
    assert_eq!(log.read_log_entries().unwrap(), ["b", "d"]);
    for bad in ["", "a\nb", "a\rb", "a\0b"] {
        assert!(
            log.compact_log_entries(&[bad.to_owned()]).is_err(),
            "{bad:?}"
        );
    }
    assert_eq!(log.read_log_entries().unwrap(), ["b", "d"]);
    log.compact_log_entries(&[]).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "");
}

#[test]
fn file_log_discards_a_stale_compaction_copy_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.log");
    fs::write(&path, "complete\n").unwrap();
    fs::write(compaction_path(&path), "partial").unwrap();
    let mut log = FileTransitionLog::open(&path).unwrap();
    assert!(!compaction_path(&path).exists());
    assert_eq!(log.read_log_entries().unwrap(), ["complete"]);
}

/// A links-backed journal at `path`, as older versions of `clink` wrote.
fn legacy_journal(path: &Path, entries: &[&str]) {
    let mut store = NamedTypesDecorator::new(path, false).unwrap();
    for entry in entries {
        store.append_log_entry(entry).unwrap();
    }
    store.flush_log().unwrap();
}

#[test]
fn the_cli_journal_lives_beside_the_database() {
    assert_eq!(
        make_transitions_log_filename("/d/db.links"),
        PathBuf::from("/d/db.transitions.log")
    );
    assert_eq!(
        make_legacy_transitions_filename("db.links"),
        PathBuf::from("db.transitions.links")
    );
}

#[test]
fn is_links_journal_tells_a_links_store_from_a_text_journal() {
    let dir = tempfile::tempdir().unwrap();
    let at = |name: &str, contents: Option<&[u8]>| {
        let path = dir.path().join(name);
        if let Some(contents) = contents {
            fs::write(&path, contents).unwrap();
        }
        path
    };
    assert!(!is_links_journal(&at("missing", None)).unwrap());
    assert!(!is_links_journal(&at("empty", Some(b""))).unwrap());
    assert!(!is_links_journal(&at("journal", Some(b"__transactions:commit:1\n"))).unwrap());
    assert!(!is_links_journal(&at("short", Some(b"__trans"))).unwrap());
    assert!(is_links_journal(&at("binary", Some(&[0, 1, 2, 3]))).unwrap());
    let legacy = dir.path().join("legacy.links");
    legacy_journal(&legacy, &["__transactions:sequence:1"]);
    assert!(is_links_journal(&legacy).unwrap());
}

#[test]
fn opening_the_journal_imports_the_legacy_store_once() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.links");
    let legacy = make_legacy_transitions_filename(&db);
    let entries = ["__transactions:sequence:5", "__transactions:applied:2"];
    legacy_journal(&legacy, &entries);
    let mut log = open_transitions_journal(&db, None, true).unwrap();
    assert_eq!(log.read_log_entries().unwrap(), entries);
    drop(log);
    assert!(!legacy.exists());
    assert!(!dir.path().join("db.transitions.names.links").exists());
    assert_eq!(lines(&make_transitions_log_filename(&db)), entries);
    let mut log = open_transitions_journal(&db, None, false).unwrap();
    assert_eq!(log.read_log_entries().unwrap(), entries);
}

#[test]
fn an_explicit_links_backed_journal_is_converted_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.links");
    let explicit = dir.path().join("custom.links");
    let entries = ["__transactions:sequence:3", "__transactions:applied:2"];
    legacy_journal(&explicit, &entries);
    drop(open_transitions_journal(&db, Some(&explicit), false).unwrap());
    assert_eq!(lines(&explicit), entries);
    assert!(!dir.path().join("custom.names.links").exists());
    // A text journal at the explicit path is opened as is.
    drop(open_transitions_journal(&db, Some(&explicit), false).unwrap());
    assert_eq!(lines(&explicit), entries);
}

#[test]
fn a_legacy_store_left_by_a_crash_is_removed_only_once_fully_imported() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.links");
    let legacy = make_legacy_transitions_filename(&db);
    let target = make_transitions_log_filename(&db);
    legacy_journal(&legacy, &["a", "b"]);
    fs::write(&target, "a\n").unwrap();
    assert!(!import_links_journal(&legacy, &target, true).unwrap());
    assert!(legacy.exists());
    fs::write(&target, "a\nb\nc\n").unwrap();
    drop(open_transitions_journal(&db, None, false).unwrap());
    assert!(!legacy.exists());
    assert_eq!(lines(&target), ["a", "b", "c"]);
}

#[test]
fn clink_keeps_its_journal_in_a_bounded_text_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("data.links");
    let clink = env!("CARGO_BIN_EXE_clink");
    for i in 1..=3 {
        let status = std::process::Command::new(clink)
            .arg("--db")
            .arg(&db)
            .args(["--transactions", "--retention", "sized:1"])
            .arg(format!("() (({i}: {i} {i}))"))
            .status()
            .unwrap();
        assert!(status.success());
    }
    let journal = dir.path().join("data.transitions.log");
    assert!(journal.exists());
    assert!(!dir.path().join("data.transitions.links").exists());
    assert!(lines(&journal)
        .iter()
        .all(|entry| entry.starts_with("__transactions:")));
}
