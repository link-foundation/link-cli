//! Restoring to a time through the optional timeline decorator.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use link_cli::engine::{
    restore_backup, revision_at, DeltaLog, Engine, EngineOptions, HistoryPolicy, MemoryLog,
    TimedLog,
};
use link_cli::{LinkError, LinksStorage, ReplaceableTransitionLogStore, TransitionLogStore};

/// A log the test can still read after an engine took it. Fails on demand.
#[derive(Clone, Default)]
struct Shared {
    log: Rc<RefCell<MemoryLog>>,
    failing: Rc<Cell<bool>>,
    durable: bool,
    fixed: bool,
}

impl Shared {
    fn entries(&self) -> Vec<String> {
        self.log.borrow_mut().read_log_entries().unwrap()
    }
    fn check(&self) -> Result<(), LinkError> {
        if self.failing.get() {
            return Err(LinkError::StorageError("injected".into()));
        }
        Ok(())
    }
}

impl TransitionLogStore for Shared {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        self.check()?;
        self.log.borrow_mut().append_log_entry(entry)
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.check()?;
        self.log.borrow_mut().read_log_entries()
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.check()
    }
    fn supports_durable_flush(&self) -> bool {
        self.durable
    }
}

impl ReplaceableTransitionLogStore for Shared {
    fn supports_replacement(&self) -> bool {
        !self.fixed
    }
    fn replace_log_entries(&mut self, entries: &[String]) -> Result<bool, LinkError> {
        *self.log.borrow_mut() = MemoryLog::default();
        for entry in entries {
            self.append_log_entry(entry)?;
        }
        Ok(true)
    }
}

type Timed = TimedLog<Shared, Shared>;

fn options() -> EngineOptions {
    EngineOptions {
        history: HistoryPolicy::All,
        ..Default::default()
    }
}

fn clock(now: &Arc<AtomicU64>) -> impl FnMut() -> u64 + Send + 'static {
    let now = now.clone();
    move || now.load(Ordering::SeqCst)
}

fn commit<L: TransitionLogStore>(engine: &Engine<u32, L>, name: &str) {
    let mut tx = engine.begin().unwrap();
    if tx.get_link(1).is_none() {
        tx.create_link(0, 0).unwrap();
    }
    tx.set_name(1, Some(name.into())).unwrap();
    tx.commit().unwrap();
}

/// Revisions 1, 2, 3 written at 100, 200, 300.
fn written(history: HistoryPolicy) -> (Engine<u32, Timed>, Shared, Shared) {
    let (journal, timeline, now) = (
        Shared::default(),
        Shared::default(),
        Arc::new(AtomicU64::new(0)),
    );
    let log = TimedLog::with_clock(journal.clone(), timeline.clone(), clock(&now)).unwrap();
    let engine = Engine::open(
        log,
        EngineOptions {
            history,
            ..Default::default()
        },
    )
    .unwrap();
    for (time, name) in [(100, "first"), (200, "second"), (300, "third")] {
        now.store(time, Ordering::SeqCst);
        commit(&engine, name);
    }
    (engine, journal, timeline)
}

fn name_after_restore(journal: &mut Shared, timeline: &mut Shared, time: u64) -> Option<String> {
    let revision = revision_at(timeline, time).unwrap();
    let mut copy = MemoryLog::default();
    assert_eq!(
        restore_backup::<u32, _, _>(journal, &mut copy, Some(revision)).unwrap(),
        revision
    );
    let restored = Engine::<u32, _>::open(copy, options()).unwrap();
    let snapshot = restored.snapshot().unwrap();
    assert_eq!(snapshot.revision, revision);
    snapshot.rows()[&1].name.clone()
}

#[test]
fn a_restore_names_a_time_and_gets_the_state_at_that_time() {
    let (_engine, mut journal, mut timeline) = written(HistoryPolicy::All);
    assert_eq!(
        timeline.entries(),
        ["1|100", "2|200", "3|300"].map(String::from)
    );
    for (time, name) in [
        (100, "first"),
        (199, "first"),
        (200, "second"),
        (299, "second"),
        (300, "third"),
        (9999, "third"),
    ] {
        assert_eq!(
            name_after_restore(&mut journal, &mut timeline, time).as_deref(),
            Some(name),
            "at {time}"
        );
    }
}

#[test]
fn a_time_before_the_first_timestamp_is_refused() {
    let (_engine, _journal, mut timeline) = written(HistoryPolicy::All);
    let refusal = revision_at(&mut timeline, 99).unwrap_err().to_string();
    assert!(refusal.contains("no revision was written"), "{refusal}");
    let refusal = revision_at(&mut MemoryLog::default(), 1).unwrap_err();
    assert!(refusal.to_string().contains("no revision was written"));
}

#[test]
fn records_are_written_unchanged_so_the_decorator_can_be_removed() {
    let (engine, journal, _timeline) = written(HistoryPolicy::All);
    drop(engine);
    let plain = Engine::<u32, _>::open(journal, options()).unwrap();
    assert_eq!(plain.snapshot().unwrap().revision, 3);
}

#[test]
fn time_never_runs_backwards_even_across_a_reopen() {
    let (journal, timeline, now) = (
        Shared::default(),
        Shared::default(),
        Arc::new(AtomicU64::new(500)),
    );
    let log = TimedLog::with_clock(journal.clone(), timeline.clone(), clock(&now)).unwrap();
    let engine = Engine::open(log, options()).unwrap();
    commit(&engine, "first");
    now.store(400, Ordering::SeqCst);
    commit(&engine, "second");
    drop(engine);

    let log = TimedLog::with_clock(journal, timeline.clone(), clock(&now)).unwrap();
    let engine = Engine::open(log, options()).unwrap();
    commit(&engine, "third");
    assert_eq!(
        timeline.entries(),
        ["1|500", "2|500", "3|500"].map(String::from)
    );
    drop(engine);
}

#[test]
fn a_revision_written_again_replaces_what_the_lost_one_claimed() {
    let mut timeline = MemoryLog::default();
    for entry in ["3|10", "4|20", "5|30", "6|40", "5|70"] {
        timeline.append_log_entry(entry).unwrap();
    }
    // What was written at 30 was lost; the revision 5 that survives is from 70.
    assert_eq!(revision_at(&mut timeline, 20).unwrap(), 4);
    assert_eq!(revision_at(&mut timeline, 69).unwrap(), 4);
    assert_eq!(revision_at(&mut timeline, 70).unwrap(), 5);
}

#[test]
fn a_damaged_timeline_is_refused_rather_than_guessed_at() {
    for entry in ["x|1", "1|x", "1"] {
        let mut timeline = MemoryLog::default();
        timeline.append_log_entry(entry).unwrap();
        let refusal = revision_at(&mut timeline, 1).unwrap_err().to_string();
        assert!(refusal.contains("malformed timeline"), "{entry}: {refusal}");
        let opened = TimedLog::new(MemoryLog::default(), timeline).err().unwrap();
        assert!(opened.to_string().contains("malformed timeline"));
    }
}

#[test]
fn a_record_without_a_revision_reaches_neither_log() {
    for record in ["other|1|2", "engine1", "engine1|x|2"] {
        let (journal, timeline) = (Shared::default(), Shared::default());
        let mut log = TimedLog::new(journal.clone(), timeline.clone()).unwrap();
        assert!(log.append_log_entry(record).is_err(), "{record}");
        assert!(journal.entries().is_empty() && timeline.entries().is_empty());
    }
}

#[test]
fn the_system_clock_is_the_default() {
    let timeline = Shared::default();
    let mut log = TimedLog::new(Shared::default(), timeline.clone()).unwrap();
    log.append_log_entry("engine1|1|2|p|c").unwrap();
    let entries = timeline.entries();
    let (_, written) = entries[0].split_once('|').unwrap();
    assert!(written.parse::<u64>().unwrap() > 1_600_000_000_000);
}

#[test]
fn a_failing_journal_or_timeline_fails_the_call() {
    let (journal, timeline) = (Shared::default(), Shared::default());
    let mut log = TimedLog::with_clock(journal.clone(), timeline.clone(), || 1).unwrap();

    timeline.failing.set(true);
    // The record lands first, so the journal can be ahead of its timeline.
    assert!(log.append_log_entry("engine1|1|2|p|c").is_err());
    assert_eq!(journal.entries().len(), 1);
    assert!(log.flush_log().is_err());
    timeline.failing.set(false);

    journal.failing.set(true);
    assert!(log.append_log_entry("engine1|2|2|p|c").is_err());
    assert!(log.flush_log().is_err());
    assert!(log.read_log_entries().is_err());
    journal.failing.set(false);
    assert!(timeline.entries().is_empty());
    log.flush_log().unwrap();
    assert_eq!(log.read_log_entries().unwrap().len(), 1);

    timeline.failing.set(true);
    let reopened = TimedLog::new(Shared::default(), timeline.clone());
    assert!(reopened.is_err());
}

#[test]
fn durability_needs_both_logs() {
    for (journal, timeline, expected) in [
        (true, true, true),
        (true, false, false),
        (false, true, false),
    ] {
        let log = TimedLog::new(
            Shared {
                durable: journal,
                ..Shared::default()
            },
            Shared {
                durable: timeline,
                ..Shared::default()
            },
        )
        .unwrap();
        assert_eq!(log.supports_durable_flush(), expected);
    }
}

#[test]
fn compaction_keeps_the_timeline_and_older_revisions_are_then_refused() {
    let (engine, mut journal, mut timeline) = written(HistoryPolicy::None);
    assert!(engine.compact_journal().unwrap());
    assert_eq!(journal.entries().len(), 1);
    assert_eq!(timeline.entries().len(), 3);

    assert_eq!(
        name_after_restore(&mut journal, &mut timeline, 300).as_deref(),
        Some("third")
    );
    let dropped = revision_at(&mut timeline, 100).unwrap();
    let refusal =
        restore_backup::<u32, _, _>(&mut journal, &mut MemoryLog::default(), Some(dropped))
            .unwrap_err();
    assert!(refusal.to_string().contains("not in the backup"));
}

#[test]
fn an_inner_log_without_replacement_keeps_compaction_unavailable() {
    let fixed = Shared {
        fixed: true,
        ..Shared::default()
    };
    let log = TimedLog::new(fixed, MemoryLog::default()).unwrap();
    let engine = Engine::<u32, _>::open(log, options()).unwrap();
    assert!(engine.compact_journal().is_err());
    assert_eq!(engine.snapshot().unwrap().revision, 0);
}

#[test]
fn it_composes_over_a_delta_journal() {
    let (inner, timeline, now) = (
        Shared::default(),
        Shared::default(),
        Arc::new(AtomicU64::new(5)),
    );
    let delta = DeltaLog::new(inner.clone(), 8).unwrap();
    let log = TimedLog::with_clock(delta, timeline.clone(), clock(&now)).unwrap();
    let engine = Engine::open(log, options()).unwrap();
    commit(&engine, "first");
    commit(&engine, "second");
    assert_eq!(timeline.entries(), ["1|5", "2|5"].map(String::from));
    assert_eq!(engine.snapshot().unwrap().revision, 2);
}

#[test]
fn a_commit_fails_when_its_timestamp_cannot_be_written() {
    let (journal, timeline) = (Shared::default(), Shared::default());
    let log = TimedLog::new(journal, timeline.clone()).unwrap();
    let engine = Engine::<u32, _>::open(log, options()).unwrap();
    timeline.failing.set(true);
    let mut tx = engine.begin().unwrap();
    tx.create_link(0, 0).unwrap();
    assert!(tx.commit().is_err());
}
