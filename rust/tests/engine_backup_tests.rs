//! Online backup: a consistent cut copied outside the commit mutex.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use link_cli::engine::{restore_backup, Engine, EngineOptions, HistoryPolicy, MemoryLog};
use link_cli::{LinkError, LinksStorage, TransitionLogStore};

type Source = Engine<u32, MemoryLog>;

fn options(history: HistoryPolicy) -> EngineOptions {
    EngineOptions {
        history,
        ..Default::default()
    }
}

/// Revisions 1..=count: revision N names link 1 "雪 N", revision 3 leaves a hole at 7.
fn engine(history: HistoryPolicy, count: u64) -> Source {
    let engine = Engine::open(MemoryLog::default(), options(history)).unwrap();
    for revision in 1..=count {
        let mut tx = engine.begin().unwrap();
        if revision == 1 {
            tx.create_link(0, 0).unwrap();
        }
        if revision == 3 {
            tx.ensure_link_created(7).unwrap();
            tx.delete_link(7).unwrap();
        }
        tx.set_name(1, Some(format!("雪 {revision}"))).unwrap();
        tx.commit().unwrap();
    }
    engine
}

fn name(engine: &Engine<u32, MemoryLog>) -> Option<String> {
    engine.snapshot().unwrap().get(1).unwrap().name.clone()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_backup_reopens_at_its_revision_and_continues_with_the_same_allocator() {
    for history in [
        HistoryPolicy::None,
        HistoryPolicy::Last(2),
        HistoryPolicy::All,
    ] {
        let source = engine(history, 6);
        let mut copy = MemoryLog::default();
        assert_eq!(source.backup_to(&mut copy, None).unwrap(), 6);
        let restored = Engine::<u32, _>::open(copy, options(history)).unwrap();
        assert_eq!(restored.snapshot().unwrap().revision, 6);
        assert_eq!(
            restored.snapshot().unwrap().rows(),
            source.snapshot().unwrap().rows()
        );
        assert_eq!(name(&restored).as_deref(), Some("雪 6"));
        let mut tx = restored.begin().unwrap();
        assert_eq!(tx.create_link(1, 0).unwrap(), 8, "the hole stays retired");
        tx.commit().unwrap();
        assert_eq!(
            source.snapshot().unwrap().revision,
            6,
            "source is untouched"
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_untouched_engine_backs_up_revision_zero() {
    let source = engine(HistoryPolicy::None, 0);
    let mut copy = MemoryLog::default();
    assert_eq!(source.backup_to(&mut copy, None).unwrap(), 0);
    let restored = Engine::<u32, _>::open(copy, EngineOptions::default()).unwrap();
    assert_eq!(restored.snapshot().unwrap().revision, 0);
    let mut tx = restored.begin().unwrap();
    assert_eq!(tx.create_link(0, 0).unwrap(), 1);
    tx.commit().unwrap();
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_retained_revision_restores_that_point_in_time_and_keeps_only_earlier_history() {
    let source = engine(HistoryPolicy::All, 6);
    let mut copy = MemoryLog::default();
    assert_eq!(source.backup_to(&mut copy, Some(2)).unwrap(), 2);
    let restored = Engine::<u32, _>::open(copy, options(HistoryPolicy::All)).unwrap();
    assert_eq!(restored.snapshot().unwrap().revision, 2);
    assert_eq!(name(&restored).as_deref(), Some("雪 2"));
    assert!(restored.snapshot_at(1).is_ok());
    assert!(restored.snapshot_at(3).is_err());
    // The allocator is the target's: revision 3 had not yet retired the hole.
    let mut tx = restored.begin().unwrap();
    assert_eq!(tx.create_link(0, 0).unwrap(), 2);
    tx.commit().unwrap();
    assert_eq!(
        source.export_checkpoint_at(6).unwrap(),
        source.export_checkpoint().unwrap()
    );
    assert_eq!(source.export_checkpoint_at(2).unwrap().len(), 3);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_unretained_revision_is_refused_and_writes_nothing() {
    for (history, revision) in [
        (HistoryPolicy::None, 5),
        (HistoryPolicy::Last(2), 3),
        (HistoryPolicy::All, 7),
    ] {
        let source = engine(history, 6);
        let mut copy = MemoryLog::default();
        let refused = source.backup_to(&mut copy, Some(revision)).unwrap_err();
        assert!(refused.to_string().contains("not retained"), "{refused}");
        assert!(copy.entries.is_empty());
        assert!(source.export_checkpoint_at(revision).is_err());
    }
    let source = engine(HistoryPolicy::None, 6);
    let mut copy = MemoryLog::default();
    assert_eq!(source.backup_to(&mut copy, Some(6)).unwrap(), 6);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_non_empty_destination_is_refused_and_left_alone() {
    let source = engine(HistoryPolicy::None, 1);
    let mut copy = MemoryLog {
        entries: vec!["existing".into()],
    };
    let refused = source.backup_to(&mut copy, None).unwrap_err();
    assert!(refused.to_string().contains("must be empty"), "{refused}");
    assert_eq!(copy.entries, ["existing"]);
}

/// Scripted destination around a real log; each hook runs before the operation.
#[derive(Default)]
struct Destination {
    inner: MemoryLog,
    during_append: Option<Box<dyn FnMut()>>,
    fail_append_at: Option<usize>,
    fail_flush: bool,
    fail_read_at: Option<usize>,
    reads: usize,
    flushes: usize,
    corrupt_after_flush: bool,
}
impl TransitionLogStore for Destination {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        if let Some(hook) = &mut self.during_append {
            hook();
        }
        if self.fail_append_at == Some(self.inner.entries.len()) {
            return Err(LinkError::Transaction("append failed".into()));
        }
        self.inner.append_log_entry(entry)
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.reads += 1;
        if self.fail_read_at == Some(self.reads) {
            return Err(LinkError::Transaction("read failed".into()));
        }
        self.inner.read_log_entries()
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        if self.fail_flush {
            return Err(LinkError::Transaction("flush failed".into()));
        }
        self.flushes += 1;
        if self.corrupt_after_flush {
            self.inner.entries.pop();
        }
        Ok(())
    }
    fn supports_durable_flush(&self) -> bool {
        false
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn commits_continue_during_the_copy_and_stay_out_of_it() {
    let source = Rc::new(engine(HistoryPolicy::All, 2));
    let commits = Rc::new(Cell::new(0));
    let (writer, counter) = (source.clone(), commits.clone());
    let mut destination = Destination {
        // This would deadlock if the commit mutex were still held.
        during_append: Some(Box::new(move || {
            let mut tx = writer.begin().unwrap();
            tx.set_name(1, Some("during".into())).unwrap();
            tx.commit().unwrap();
            counter.set(counter.get() + 1);
        })),
        ..Default::default()
    };
    assert_eq!(source.backup_to(&mut destination, None).unwrap(), 2);
    assert_eq!(commits.get(), 3, "one commit before each of three records");
    assert_eq!(source.snapshot().unwrap().revision, 5);
    let restored = Engine::<u32, _>::open(destination.inner, options(HistoryPolicy::All)).unwrap();
    assert_eq!(restored.snapshot().unwrap().revision, 2);
    assert_eq!(name(&restored).as_deref(), Some("雪 2"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_failed_destination_neither_blocks_nor_fences_the_engine() {
    let failures: [(&str, Destination); 5] = [
        (
            "read failed",
            Destination {
                fail_read_at: Some(1),
                ..Default::default()
            },
        ),
        (
            "append failed",
            Destination {
                fail_append_at: Some(1),
                ..Default::default()
            },
        ),
        (
            "flush failed",
            Destination {
                fail_flush: true,
                ..Default::default()
            },
        ),
        (
            "read failed",
            Destination {
                fail_read_at: Some(2),
                ..Default::default()
            },
        ),
        (
            "verification failed",
            Destination {
                corrupt_after_flush: true,
                ..Default::default()
            },
        ),
    ];
    for (expected, mut destination) in failures {
        let source = engine(HistoryPolicy::All, 2);
        let refused = source.backup_to(&mut destination, None).unwrap_err();
        assert!(refused.to_string().contains(expected), "{refused}");
        let mut tx = source.begin().unwrap();
        tx.set_name(1, Some("after".into())).unwrap();
        assert_eq!(tx.commit().unwrap(), 3);
        // A retry on a fresh destination succeeds and sees the later commit.
        let mut copy = MemoryLog::default();
        assert_eq!(source.backup_to(&mut copy, None).unwrap(), 3);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn the_copy_is_flushed_and_private_writes_stay_out_of_it() {
    let source = engine(HistoryPolicy::None, 1);
    let mut pending = source.begin().unwrap();
    pending.create_link(0, 0).unwrap();
    let mut destination = Destination::default();
    assert_eq!(source.backup_to(&mut destination, None).unwrap(), 1);
    assert_eq!(destination.flushes, 1);
    pending.commit().unwrap();
    let restored = Engine::<u32, _>::open(destination.inner, EngineOptions::default()).unwrap();
    assert_eq!(restored.snapshot().unwrap().links().len(), 1);
}

/// Shares a failure flag with the test, so the source log can fail its flush later.
#[derive(Clone, Default)]
struct FailingLog {
    records: Rc<RefCell<Vec<String>>>,
    fail: Rc<Cell<bool>>,
}
impl TransitionLogStore for FailingLog {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        self.records.borrow_mut().push(entry.to_owned());
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.records.borrow().clone())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        if self.fail.get() {
            Err(LinkError::Transaction("source flush failed".into()))
        } else {
            Ok(())
        }
    }
    fn supports_durable_flush(&self) -> bool {
        false
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_engine_with_an_unknown_journal_outcome_refuses_to_back_up() {
    let log = FailingLog::default();
    let source = Engine::<u32, _>::open(log.clone(), EngineOptions::default()).unwrap();
    let mut tx = source.begin().unwrap();
    tx.create_link(0, 0).unwrap();
    log.fail.set(true);
    assert!(tx.commit().is_err());
    let mut copy = MemoryLog::default();
    let refused = source.backup_to(&mut copy, None).unwrap_err();
    assert!(refused.to_string().contains("reopen"), "{refused}");
    assert!(copy.entries.is_empty());
    assert!(source.export_checkpoint_at(0).is_err());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_backup_survives_reopening_from_a_durable_file() {
    let directory = std::env::temp_dir().join(format!("engine-backup-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("backup.log");
    let _ = std::fs::remove_file(&path);
    let source = engine(HistoryPolicy::Last(2), 4);
    let mut file = link_cli::FileTransitionLog::open(&path).unwrap();
    assert_eq!(source.backup_to(&mut file, Some(3)).unwrap(), 3);
    drop(file);
    let file = link_cli::FileTransitionLog::open(&path).unwrap();
    let restored = Engine::<u32, _>::open(file, EngineOptions::default()).unwrap();
    assert_eq!(restored.snapshot().unwrap().revision, 3);
    assert_eq!(
        restored.snapshot().unwrap().get(1).unwrap().name.as_deref(),
        Some("雪 3")
    );
    drop(restored);
    std::fs::remove_dir_all(&directory).unwrap();
}

/// A later commit on `source` that names link 1 and deletes what `delete` holds.
fn commit(source: &Source, text: &str, delete: Option<u32>) {
    let mut tx = source.begin().unwrap();
    tx.set_name(1, Some(text.into())).unwrap();
    if let Some(index) = delete {
        tx.delete_link(index).unwrap();
    }
    tx.commit().unwrap();
}

fn refused<D: TransitionLogStore>(source: &Source, log: &mut D, message: &str) {
    let refused = source.backup_incremental_to(log).unwrap_err().to_string();
    assert!(refused.contains(message), "{refused}");
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_incremental_backup_appends_only_changed_rows_and_reopens_as_the_source() {
    let source = engine(HistoryPolicy::All, 2);
    let mut tx = source.begin().unwrap();
    for _ in 0..50 {
        tx.create_link(1, 1).unwrap();
    }
    tx.commit().unwrap();
    let mut copy = MemoryLog::default();
    assert_eq!(source.backup_to(&mut copy, None).unwrap(), 3);
    let full = copy.entries.clone();

    commit(&source, "four", None);
    commit(&source, "five", Some(2));
    assert_eq!(source.backup_incremental_to(&mut copy).unwrap(), 5);
    assert_eq!(copy.entries[..full.len()], full, "earlier records stay");
    let added = &copy.entries[full.len()..];
    assert_eq!(added.len(), 2);
    assert!(added.iter().all(|record| record.starts_with("engine2|")));
    assert!(added[0].len() * 10 < full.last().unwrap().len());

    // Called again it adds nothing, and later commits continue the same copy.
    assert_eq!(source.backup_incremental_to(&mut copy).unwrap(), 5);
    assert_eq!(copy.entries.len(), full.len() + 2);
    commit(&source, "six", None);
    assert_eq!(source.backup_incremental_to(&mut copy).unwrap(), 6);

    let restored = Engine::<u32, _>::open(copy, options(HistoryPolicy::All)).unwrap();
    assert_eq!(restored.snapshot().unwrap().revision, 6);
    assert_eq!(
        restored.snapshot().unwrap().rows(),
        source.snapshot().unwrap().rows()
    );
    assert_eq!(
        restored.snapshot_at(4).unwrap().rows(),
        source.snapshot_at(4).unwrap().rows()
    );
    let (mut mine, mut theirs) = (restored.begin().unwrap(), source.begin().unwrap());
    assert_eq!(
        mine.create_link(0, 0).unwrap(),
        theirs.create_link(0, 0).unwrap(),
        "the allocator continues at the same address"
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_empty_destination_gets_a_full_copy() {
    let source = engine(HistoryPolicy::All, 3);
    let mut copy = MemoryLog::default();
    assert_eq!(source.backup_incremental_to(&mut copy).unwrap(), 3);
    assert_eq!(copy.entries, source.export_checkpoint().unwrap());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_copy_older_than_the_retained_history_is_refused_and_left_alone() {
    // A bounded history keeps a recent copy usable and refuses an old one.
    let source = engine(HistoryPolicy::Last(2), 3);
    let mut recent = MemoryLog::default();
    source.backup_to(&mut recent, None).unwrap();
    commit(&source, "four", None);
    assert_eq!(source.backup_incremental_to(&mut recent).unwrap(), 4);
    let before = recent.entries.clone();
    for text in ["five", "six", "seven"] {
        commit(&source, text, None);
    }
    refused(&source, &mut recent, "revision is not retained");
    assert_eq!(recent.entries, before);

    // Without history only a copy of the current revision can continue.
    let source = engine(HistoryPolicy::None, 3);
    let mut copy = MemoryLog::default();
    source.backup_to(&mut copy, None).unwrap();
    assert_eq!(source.backup_incremental_to(&mut copy).unwrap(), 3);
    commit(&source, "four", None);
    refused(&source, &mut copy, "revision is not retained");
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_copy_of_another_timeline_is_refused_and_left_alone() {
    let source = engine(HistoryPolicy::All, 2);

    // Different rows at a revision both have.
    let other = engine(HistoryPolicy::All, 1);
    commit(&other, "elsewhere", None);
    let mut diverged = MemoryLog::default();
    other.backup_to(&mut diverged, None).unwrap();
    let before = diverged.entries.clone();
    refused(&source, &mut diverged, "not a copy of this engine");
    assert_eq!(diverged.entries, before);

    // The same rows, but the allocator moved further.
    let plain = Engine::<u32, _>::open(MemoryLog::default(), options(HistoryPolicy::All)).unwrap();
    let moved = Engine::<u32, _>::open(MemoryLog::default(), options(HistoryPolicy::All)).unwrap();
    for (engine, hole) in [(&plain, false), (&moved, true)] {
        let mut tx = engine.begin().unwrap();
        tx.create_link(0, 0).unwrap();
        if hole {
            tx.ensure_link_created(7).unwrap();
            tx.delete_link(7).unwrap();
        }
        tx.commit().unwrap();
    }
    let mut allocator = MemoryLog::default();
    moved.backup_to(&mut allocator, None).unwrap();
    refused(&plain, &mut allocator, "not a copy of this engine");

    // A copy from further along than this engine.
    let mut ahead = MemoryLog::default();
    engine(HistoryPolicy::All, 5)
        .backup_to(&mut ahead, None)
        .unwrap();
    refused(&source, &mut ahead, "revision is not retained");
    let none = engine(HistoryPolicy::None, 2);
    refused(&none, &mut ahead, "revision is not retained");

    // Text that is not a journal.
    let mut broken = MemoryLog::default();
    broken.append_log_entry("not a record").unwrap();
    assert!(source.backup_incremental_to(&mut broken).is_err());
    assert_eq!(broken.entries, ["not a record"]);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_live_journal_of_this_engine_continues_as_a_copy() {
    // A journal that has no checkpoint record replays from revision zero.
    let log = FailingLog::default();
    let source = Engine::<u32, _>::open(log.clone(), options(HistoryPolicy::All)).unwrap();
    let write = |text: &str| {
        let mut tx = source.begin().unwrap();
        tx.ensure_link_created(1).unwrap();
        tx.set_name(1, Some(text.into())).unwrap();
        tx.commit().unwrap();
    };
    write("one");
    write("two");
    let mut copy = MemoryLog::default();
    for record in log.records.borrow().iter() {
        copy.append_log_entry(record).unwrap();
    }
    write("three");
    assert_eq!(source.backup_incremental_to(&mut copy).unwrap(), 3);
    let restored = Engine::<u32, _>::open(copy, options(HistoryPolicy::All)).unwrap();
    assert_eq!(name(&restored).as_deref(), Some("three"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_failed_increment_can_be_continued_and_does_not_fence_the_engine() {
    let source = engine(HistoryPolicy::All, 2);
    let mut destination = Destination::default();
    source.backup_to(&mut destination, None).unwrap();
    for text in ["three", "four"] {
        commit(&source, text, None);
    }
    let held = destination.inner.entries.len();

    destination.fail_append_at = Some(held + 1);
    refused(&source, &mut destination, "append failed");
    assert_eq!(destination.inner.entries.len(), held + 1);
    destination.fail_append_at = None;

    destination.fail_flush = true;
    refused(&source, &mut destination, "flush failed");
    destination.fail_flush = false;

    // The read before and the read after the copy.
    destination.reads = 0;
    destination.fail_read_at = Some(1);
    refused(&source, &mut destination, "read failed");
    destination.reads = 0;
    destination.fail_read_at = Some(2);
    refused(&source, &mut destination, "read failed");
    destination.fail_read_at = None;

    assert_eq!(source.backup_incremental_to(&mut destination).unwrap(), 4);

    commit(&source, "five", None);
    destination.corrupt_after_flush = true;
    refused(&source, &mut destination, "verification failed");
    destination.corrupt_after_flush = false;

    commit(&source, "six", None);
    assert_eq!(source.backup_incremental_to(&mut destination).unwrap(), 6);
    let restored = Engine::<u32, _>::open(destination.inner, options(HistoryPolicy::All)).unwrap();
    assert_eq!(name(&restored).as_deref(), Some("six"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn commits_continue_during_an_increment_and_stay_out_of_it() {
    let source = Rc::new(engine(HistoryPolicy::All, 2));
    let mut destination = Destination::default();
    source.backup_to(&mut destination, None).unwrap();
    commit(&source, "three", None);
    let writer = source.clone();
    // This would deadlock if the commit mutex were still held.
    destination.during_append = Some(Box::new(move || commit(&writer, "during", None)));
    assert_eq!(source.backup_incremental_to(&mut destination).unwrap(), 3);
    assert_eq!(source.snapshot().unwrap().revision, 4);
    let restored = Engine::<u32, _>::open(destination.inner, options(HistoryPolicy::All)).unwrap();
    assert_eq!(name(&restored).as_deref(), Some("three"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_engine_with_an_unknown_journal_outcome_refuses_an_increment() {
    let log = FailingLog::default();
    let source = Engine::<u32, _>::open(log.clone(), EngineOptions::default()).unwrap();
    let mut copy = MemoryLog::default();
    source.backup_to(&mut copy, None).unwrap();
    let before = copy.entries.clone();
    let mut tx = source.begin().unwrap();
    tx.create_link(0, 0).unwrap();
    log.fail.set(true);
    assert!(tx.commit().is_err());
    let refused = source.backup_incremental_to(&mut copy).unwrap_err();
    assert!(refused.to_string().contains("reopen"), "{refused}");
    assert_eq!(copy.entries, before);
}

/// A chain of two increments on a full copy, with revision 3 as the full copy's end.
fn archive() -> (Source, MemoryLog) {
    let source = engine(HistoryPolicy::All, 3);
    let mut archive = MemoryLog::default();
    source.backup_incremental_to(&mut archive).unwrap();
    for text in ["fourth", "fifth"] {
        commit(&source, text, None);
        source.backup_incremental_to(&mut archive).unwrap();
    }
    (source, archive)
}

fn restore(archive: &mut MemoryLog, revision: Option<u64>) -> Result<MemoryLog, LinkError> {
    let mut copy = MemoryLog::default();
    restore_backup::<u32, _, _>(archive, &mut copy, revision)?;
    Ok(copy)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_archive_restores_every_revision_it_holds_as_the_source_had_it() {
    let (source, mut archive) = archive();
    for revision in 0..=5 {
        let mut copy = MemoryLog::default();
        assert_eq!(
            restore_backup::<u32, _, _>(&mut archive, &mut copy, Some(revision)).unwrap(),
            revision
        );
        let restored = Source::open(copy, options(HistoryPolicy::All)).unwrap();
        assert_eq!(restored.snapshot().unwrap().revision, revision);
        assert_eq!(
            restored.snapshot().unwrap().rows(),
            source.snapshot_at(revision).unwrap().rows()
        );
    }
    let whole = archive.entries.clone();
    assert_eq!(restore(&mut archive, None).unwrap().entries, whole);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_restored_engine_starts_a_new_timeline_with_that_revisions_allocator() {
    let (source, mut archive) = archive();
    let early = Source::open(
        restore(&mut archive, Some(2)).unwrap(),
        options(HistoryPolicy::All),
    )
    .unwrap();
    let mut tx = early.begin().unwrap();
    assert_eq!(tx.create_link(0, 0).unwrap(), 2);
    tx.commit().unwrap();
    assert_eq!(early.snapshot().unwrap().revision, 3);
    assert_eq!(source.snapshot().unwrap().revision, 5);
    assert_eq!(name(&source).as_deref(), Some("fifth"));

    // The hole left at revision 3 is part of the allocator that revision restores.
    let later = Source::open(
        restore(&mut archive, Some(3)).unwrap(),
        options(HistoryPolicy::All),
    )
    .unwrap();
    let mut tx = later.begin().unwrap();
    assert_eq!(tx.create_link(0, 0).unwrap(), 8);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_restore_can_end_just_before_damage_and_refuses_to_cross_it() {
    let (_source, mut archive) = archive();
    assert_eq!(archive.entries.len(), 6);
    archive.entries[5] = archive.entries[5].replacen('|', "|9", 1);
    let copy = restore(&mut archive, Some(4)).unwrap();
    assert_eq!(copy.entries, archive.entries[..5]);
    assert_eq!(
        Source::open(copy, options(HistoryPolicy::All))
            .unwrap()
            .snapshot()
            .unwrap()
            .revision,
        4
    );
    for revision in [Some(5), None] {
        let mut copy = MemoryLog::default();
        assert!(restore_backup::<u32, _, _>(&mut archive, &mut copy, revision).is_err());
        assert!(copy.entries.is_empty());
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_revision_the_archive_does_not_hold_is_refused_and_writes_nothing() {
    let (_source, mut full) = archive();
    let evicted = engine(HistoryPolicy::Last(2), 5);
    let mut limited = MemoryLog::default();
    evicted.backup_to(&mut limited, None).unwrap();
    let refused = |archive: &mut MemoryLog, revision| {
        let mut copy = MemoryLog::default();
        let refusal = restore_backup::<u32, _, _>(archive, &mut copy, Some(revision)).unwrap_err();
        assert!(refusal.to_string().contains("not in the backup"));
        assert!(copy.entries.is_empty());
    };
    refused(&mut full, 6);
    refused(&mut limited, 3);
    refused(&mut limited, 0);
    // The oldest revision a bounded archive still holds is its checkpoint.
    let first = restore(&mut limited, Some(4)).unwrap();
    assert_eq!(first.entries.len(), 1);
    assert!(first.entries[0].starts_with("engine3|"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_empty_archive_restores_only_the_empty_engine() {
    let mut archive = MemoryLog::default();
    assert!(restore(&mut archive, None).unwrap().entries.is_empty());
    assert!(restore(&mut archive, Some(0)).unwrap().entries.is_empty());
    assert!(restore(&mut archive, Some(1)).is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_untouched_engines_checkpoint_restores_revision_zero() {
    let source = engine(HistoryPolicy::All, 0);
    let mut archive = MemoryLog::default();
    source.backup_to(&mut archive, None).unwrap();
    assert_eq!(archive.entries.len(), 1);
    assert_eq!(restore(&mut archive, Some(0)).unwrap().entries.len(), 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_non_empty_restore_destination_is_refused_and_left_alone() {
    let (_source, mut archive) = archive();
    let mut copy = MemoryLog::default();
    copy.append_log_entry("kept").unwrap();
    let refused = restore_backup::<u32, _, _>(&mut archive, &mut copy, None).unwrap_err();
    assert!(refused.to_string().contains("must be empty"));
    assert_eq!(copy.entries, ["kept"]);
}

fn failing_destination() -> Destination {
    Destination {
        inner: MemoryLog::default(),
        during_append: None,
        fail_append_at: None,
        fail_flush: false,
        fail_read_at: None,
        reads: 0,
        flushes: 0,
        corrupt_after_flush: false,
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_failure_of_a_restores_logs_is_reported() {
    let (_source, mut archive) = archive();
    let attempt = |archive: &mut MemoryLog, change: fn(&mut Destination)| {
        let mut copy = failing_destination();
        change(&mut copy);
        let outcome = restore_backup::<u32, _, _>(archive, &mut copy, None);
        (outcome.unwrap_err().to_string(), copy)
    };
    let (message, copy) = attempt(&mut archive, |copy| copy.fail_read_at = Some(1));
    assert!(message.contains("read failed"));
    assert!(copy.inner.entries.is_empty());
    let (message, _) = attempt(&mut archive, |copy| copy.fail_append_at = Some(1));
    assert!(message.contains("append failed"));
    let (message, _) = attempt(&mut archive, |copy| copy.fail_flush = true);
    assert!(message.contains("flush failed"));
    let (message, _) = attempt(&mut archive, |copy| copy.fail_read_at = Some(2));
    assert!(message.contains("read failed"));
    let (message, _) = attempt(&mut archive, |copy| copy.corrupt_after_flush = true);
    assert!(message.contains("restore verification failed"));

    let mut unreadable = failing_destination();
    unreadable.fail_read_at = Some(1);
    let refused =
        restore_backup::<u32, _, _>(&mut unreadable, &mut MemoryLog::default(), None).unwrap_err();
    assert!(refused.to_string().contains("read failed"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_live_journal_restores_like_an_archive() {
    // A journal that has no checkpoint record replays from revision zero.
    let log = FailingLog::default();
    let source = Engine::<u32, _>::open(log.clone(), options(HistoryPolicy::All)).unwrap();
    for text in ["one", "two", "three"] {
        let mut tx = source.begin().unwrap();
        tx.ensure_link_created(1).unwrap();
        tx.set_name(1, Some(text.into())).unwrap();
        tx.commit().unwrap();
    }
    let mut journal = MemoryLog {
        entries: log.records.borrow().clone(),
    };
    let copy = restore(&mut journal, Some(2)).unwrap();
    assert_eq!(copy.entries, journal.entries[..2]);
    let restored = Engine::<u32, _>::open(copy, options(HistoryPolicy::All)).unwrap();
    assert_eq!(name(&restored).as_deref(), Some("two"));
    assert!(restore(&mut journal, Some(0)).unwrap().entries.is_empty());
}
