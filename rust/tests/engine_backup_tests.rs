//! Online backup: a consistent cut copied outside the commit mutex.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use link_cli::engine::{Engine, EngineOptions, HistoryPolicy, MemoryLog};
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
