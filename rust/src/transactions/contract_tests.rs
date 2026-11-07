//! Private invariants no public sequence can violate on its own: these tests
//! set the state directly and check the guard that would catch it.
use super::*;
use crate::engine::MemoryLog;
use crate::LinkStorage;

type Tx = GenericTransactionsDecorator<u32, LinkStorage, MemoryLog>;

fn open() -> Tx {
    GenericTransactionsDecorator::new(
        LinkStorage::in_memory(false),
        MemoryLog::default(),
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap()
}

fn transition(
    kind: TransitionKind,
    before: GenericDoubletLink<u32>,
    after: GenericDoubletLink<u32>,
) -> GenericTransition<u32> {
    GenericTransition {
        transaction_id: 1,
        sequence: 1,
        timestamp_ms: 0,
        kind,
        before,
        after,
    }
}

fn row(index: u32) -> GenericDoubletLink<u32> {
    GenericDoubletLink::new(index, 7, 8)
}

fn empty() -> GenericDoubletLink<u32> {
    GenericDoubletLink::empty()
}

/// Three committed rows whose applied markers were lost, as a crash could leave.
fn committed_without_markers() -> Tx {
    let mut tx = open();
    for _ in 0..3 {
        tx.create(10, 20).unwrap();
    }
    tx.applied.clear();
    tx
}

#[test]
fn sized_retention_applies_a_committed_head_before_dropping_it() {
    let mut tx = committed_without_markers();
    tx.set_retention_policy(LogRetentionPolicy::Sized { max_transitions: 1 });
    tx.enforce_retention().unwrap();
    assert_eq!(tx.log.len(), 1);
    assert_eq!(tx.applied.len(), 2);
    assert_eq!(tx.all().len(), 3);
}

#[test]
fn sized_retention_keeps_a_pending_head() {
    let mut tx = open();
    tx.begin_transaction().unwrap();
    for _ in 0..2 {
        tx.create(10, 20).unwrap();
    }
    tx.set_retention_policy(LogRetentionPolicy::Sized { max_transitions: 1 });
    tx.enforce_retention().unwrap();
    assert_eq!(tx.log.len(), 2);
}

#[test]
fn zero_sized_retention_disables_trimming() {
    let mut tx = committed_without_markers();
    tx.set_retention_policy(LogRetentionPolicy::Sized { max_transitions: 0 });
    tx.enforce_retention().unwrap();
    assert_eq!(tx.log.len(), 3);
}

#[test]
fn chunked_retention_applies_committed_records_before_archiving() {
    let directory = tempfile::tempdir().unwrap();
    let mut tx = committed_without_markers();
    tx.set_retention_policy(LogRetentionPolicy::Chunked {
        chunk_size: 2,
        archive_directory: directory.path().to_path_buf(),
    });
    tx.enforce_retention().unwrap();
    assert_eq!(tx.log.len(), 1);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    assert_eq!(tx.all().len(), 3);
}

#[test]
fn chunked_retention_keeps_short_disabled_and_pending_chunks() {
    let directory = tempfile::tempdir().unwrap();
    let chunked = |chunk_size| LogRetentionPolicy::Chunked {
        chunk_size,
        archive_directory: directory.path().to_path_buf(),
    };
    let mut tx = open();
    tx.begin_transaction().unwrap();
    for _ in 0..2 {
        tx.create(10, 20).unwrap();
    }
    for chunk_size in [0, 3, 2] {
        tx.set_retention_policy(chunked(chunk_size));
        tx.enforce_retention().unwrap();
        assert_eq!(tx.log.len(), 2, "chunk size {chunk_size}");
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn chunked_retention_reports_an_unusable_archive_path() {
    let directory = tempfile::tempdir().unwrap();
    let blocked = directory.path().join("file");
    std::fs::write(&blocked, b"").unwrap();
    let mut tx = committed_without_markers();
    tx.set_retention_policy(LogRetentionPolicy::Chunked {
        chunk_size: 1,
        archive_directory: blocked,
    });
    let error = tx.enforce_retention().unwrap_err();
    assert!(error.to_string().contains("archive"), "{error}");
}

#[test]
fn replay_skips_records_with_no_address_or_no_row() {
    let mut tx = open();
    let cases = [
        transition(TransitionKind::Create, empty(), empty()),
        transition(TransitionKind::Update, empty(), empty()),
        transition(TransitionKind::Update, row(9), row(9)),
        transition(TransitionKind::Delete, empty(), empty()),
        transition(TransitionKind::Delete, row(9), empty()),
    ];
    for case in &cases {
        tx.apply_transition(case).unwrap();
    }
    assert!(tx.all().is_empty());
}

#[test]
fn undo_skips_records_with_no_address_or_no_row() {
    let mut tx = open();
    let cases = [
        transition(TransitionKind::Create, empty(), empty()),
        transition(TransitionKind::Create, empty(), row(9)),
        transition(TransitionKind::Update, empty(), empty()),
        transition(TransitionKind::Update, row(9), row(9)),
        transition(TransitionKind::Delete, empty(), empty()),
    ];
    for case in &cases {
        tx.revert_transition(case).unwrap();
    }
    assert!(tx.all().is_empty());
}

#[test]
fn recording_requires_an_open_transaction() {
    let mut tx = open();
    let record = JournalRecord::from(transition(TransitionKind::Create, empty(), row(1)));
    let error = tx
        .record_records(std::iter::once(record), false)
        .unwrap_err();
    assert!(
        error.to_string().contains("missing open transaction"),
        "{error}"
    );
}

#[test]
fn version_replay_requires_ownership_and_no_open_transaction() {
    let mut tx = open();
    let record = JournalRecord::from(transition(TransitionKind::Create, empty(), row(1)));
    let error = tx.replay_version_transition(&record, false).unwrap_err();
    assert!(error.to_string().contains("does not own replay"), "{error}");
    assert!(tx.flush_version_replay().is_err());

    tx.begin_transaction().unwrap();
    let error = tx.begin_version_replay().unwrap_err();
    assert!(error.to_string().contains("still owns"), "{error}");
    tx.rollback().unwrap();

    tx.begin_version_replay().unwrap();
    tx.replay_version_transition(&record, false).unwrap();
    assert!(tx.get(1).is_some());
    tx.replay_version_transition(&record, true).unwrap();
    assert!(tx.get(1).is_none());
    tx.finish_version_replay();
}

#[test]
fn commit_without_an_owner_changes_nothing() {
    let mut tx = open();
    tx.commit_current().unwrap();
    assert!(tx.journal().is_empty());
}

#[test]
fn malformed_retention_and_transition_text_is_refused() {
    for spec in [
        "chunked:0:/tmp/x",
        "chunked:5:",
        "chunked:5",
        "chunked:x:/tmp",
    ] {
        assert!(LogRetentionPolicy::parse(spec).is_err(), "{spec}");
    }
    assert_eq!(TransitionKind::from_u8(3), None);
    for text in [
        "",
        "v1",
        "v2|1|2|3|0|1,2|1,2,3",
        "v2|1|2|3|9|1,2,3|1,2,3",
        "v2|1|2|3|0|1,2,3|1,x,3",
    ] {
        assert!(
            GenericTransition::<u32>::try_parse(text).is_none(),
            "{text}"
        );
    }
    assert_eq!(
        GenericDoubletLink::from(GenericLink::new(1u32, 2, 3)),
        GenericDoubletLink::new(1, 2, 3)
    );
}

#[test]
fn undo_image_parsing_refuses_other_shapes() {
    for text in ["", "v3|undo", "v2|undo|1|0|a|b", "v3|redo|1|0|a|b"] {
        assert!(undo_image::UndoImage::<u32>::parse(text).is_err(), "{text}");
    }
}
