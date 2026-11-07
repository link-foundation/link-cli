//! The optional worker compacts while commits continue, and reports its failure.
#![cfg(not(target_arch = "wasm32"))]
use link_cli::engine::{BackgroundCompaction, Engine, EngineOptions};
use link_cli::{LinkError, LinksStorage, ReplaceableTransitionLogStore, TransitionLogStore};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Default, PartialEq)]
enum Fault {
    #[default]
    None,
    Fail,
    Panic,
    Unreplaceable,
}
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<String>>>, Fault);
impl TransitionLogStore for Log {
    fn flush_log(&mut self) -> Result<(), LinkError> {
        Ok(())
    }
    fn append_log_entry(&mut self, row: &str) -> Result<(), LinkError> {
        self.0.lock().unwrap().push(row.into());
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.0.lock().unwrap().clone())
    }
}
impl ReplaceableTransitionLogStore for Log {
    fn supports_replacement(&self) -> bool {
        self.1 != Fault::Unreplaceable
    }
    fn replace_log_entries(&mut self, rows: &[String]) -> Result<bool, LinkError> {
        match self.1 {
            Fault::Panic => panic!("injected replacement panic"),
            Fault::Fail => Err(LinkError::Transaction("injected failure".into())),
            _ => {
                *self.0.lock().unwrap() = rows.to_vec();
                Ok(true)
            }
        }
    }
}

const PERIOD: Duration = Duration::from_millis(1);
const PATIENCE: Duration = Duration::from_secs(30);

fn open(fault: Fault) -> (Engine<u32, Log>, Log) {
    let log = Log(Arc::default(), fault);
    (
        Engine::open(log.clone(), EngineOptions::default()).unwrap(),
        log,
    )
}
fn commit(engine: &Engine<u32, Log>) -> Result<(), LinkError> {
    let mut tx = engine.begin()?;
    tx.create_link(0, 0)?;
    tx.commit().map(drop)
}
fn until(mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < PATIENCE, "the worker made no progress");
        std::thread::sleep(PERIOD);
    }
}

#[test]
fn the_worker_compacts_a_long_journal_and_the_state_survives_reopening() {
    let (engine, log) = open(Fault::None);
    for _ in 0..4 {
        commit(&engine).unwrap();
    }
    let long = log.0.lock().unwrap().len();
    let worker = BackgroundCompaction::start(engine.clone(), 2, PERIOD).unwrap();
    until(|| log.0.lock().unwrap().len() < long);
    worker.stop().unwrap();
    let snapshot = engine.snapshot().unwrap();
    let reopened = Engine::<u32, _>::open(log, EngineOptions::default()).unwrap();
    assert_eq!(reopened.snapshot().unwrap().rows(), snapshot.rows());
    assert_eq!(reopened.snapshot().unwrap().revision, 4);
}

#[test]
fn commits_continue_while_the_worker_runs() {
    let (engine, log) = open(Fault::None);
    let worker = BackgroundCompaction::start(engine.clone(), 1, PERIOD).unwrap();
    for _ in 0..64 {
        commit(&engine).unwrap();
    }
    worker.stop().unwrap();
    assert_eq!(engine.snapshot().unwrap().revision, 64);
    let reopened = Engine::<u32, _>::open(log, EngineOptions::default()).unwrap();
    assert_eq!(reopened.snapshot().unwrap().revision, 64);
}

#[test]
fn a_wrong_selection_is_refused_before_a_worker_starts() {
    let (engine, _) = open(Fault::None);
    assert!(BackgroundCompaction::start(engine.clone(), 0, PERIOD).is_err());
    assert!(BackgroundCompaction::start(engine, 1, Duration::ZERO).is_err());
    let (unreplaceable, _) = open(Fault::Unreplaceable);
    assert!(BackgroundCompaction::start(unreplaceable, 1, PERIOD).is_err());
}

#[test]
fn a_failed_pass_ends_the_worker_fences_the_engine_and_is_reported() {
    let (engine, _) = open(Fault::Fail);
    commit(&engine).unwrap();
    let worker = BackgroundCompaction::start(engine.clone(), 1, PERIOD).unwrap();
    until(|| commit(&engine).is_err());
    assert!(worker
        .stop()
        .unwrap_err()
        .to_string()
        .contains("injected failure"));
}

#[test]
fn a_panicking_pass_is_reported_by_stop() {
    let (engine, _) = open(Fault::Panic);
    commit(&engine).unwrap();
    let worker = BackgroundCompaction::start(engine.clone(), 1, PERIOD).unwrap();
    until(|| engine.snapshot().is_err());
    assert!(worker.stop().unwrap_err().to_string().contains("panicked"));
}

#[test]
fn dropping_the_handle_stops_the_worker() {
    let (engine, log) = open(Fault::None);
    commit(&engine).unwrap();
    drop(BackgroundCompaction::start(engine.clone(), u64::MAX, PERIOD).unwrap());
    let records = log.0.lock().unwrap().len();
    commit(&engine).unwrap();
    assert_eq!(log.0.lock().unwrap().len(), records + 1);
}

#[test]
fn dropping_the_handle_discards_the_failure_of_an_ended_worker() {
    let (engine, _) = open(Fault::Fail);
    commit(&engine).unwrap();
    let worker = BackgroundCompaction::start(engine.clone(), 1, PERIOD).unwrap();
    until(|| commit(&engine).is_err());
    drop(worker);
}
