use link_cli::engine::{Engine, EngineOptions, MemoryLog};
use link_cli::{LinkError, LinksStorage, TransitionLogStore};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[derive(Clone, Default)]
struct FaultLog {
    records: Arc<Mutex<MemoryLog>>,
    fail: Arc<AtomicBool>,
}
impl TransitionLogStore for FaultLog {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        self.records.lock().unwrap().append_log_entry(entry)
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.records.lock().unwrap().read_log_entries()
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        if self.fail.load(Ordering::SeqCst) {
            Err(LinkError::Transaction("injected flush failure".into()))
        } else {
            Ok(())
        }
    }
}

#[test]
fn failed_merge_is_indeterminate_and_replay_recovers_it_once() {
    let log = FaultLog::default();
    let parent = Engine::<u32, _>::open(log.clone(), EngineOptions::default()).unwrap();
    let branch_log = MemoryLog::default();
    let mut branch = parent.fork(branch_log, EngineOptions::default()).unwrap();
    let descriptor = branch.merge_base_record();
    let mut remote = branch.engine().begin().unwrap();
    remote.create_link(0, 0).unwrap();
    remote.commit().unwrap();
    drop(remote);
    log.fail.store(true, Ordering::SeqCst);
    assert!(branch
        .merge()
        .unwrap_err()
        .to_string()
        .contains("indeterminate"));
    assert!(parent.snapshot().is_err());
    assert!(parent
        .fork(MemoryLog::default(), EngineOptions::default())
        .is_err());
    let branch_log = branch.into_engine().into_log().ok().unwrap();
    assert!(parent
        .resume_branch(branch_log.clone(), EngineOptions::default(), &descriptor)
        .is_err());
    log.fail.store(false, Ordering::SeqCst);
    let reopened = Engine::<u32, _>::open(log, EngineOptions::default()).unwrap();
    assert_eq!(reopened.snapshot().unwrap().links().len(), 1);
    let mut branch = reopened
        .resume_branch(branch_log, EngineOptions::default(), &descriptor)
        .unwrap();
    assert_eq!(
        branch.merge().unwrap(),
        1,
        "recovery must not duplicate a completed merge"
    );
}

#[test]
fn failed_branch_seed_does_not_modify_parent_and_unfinished_writes_do_not_merge() {
    let parent = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let failed = FaultLog::default();
    failed.fail.store(true, Ordering::SeqCst);
    assert!(parent.fork(failed, EngineOptions::default()).is_err());
    assert_eq!(parent.snapshot().unwrap().revision, 0);
    let mut branch = parent
        .fork(MemoryLog::default(), EngineOptions::default())
        .unwrap();
    let mut unfinished = branch.engine().begin().unwrap();
    unfinished.create_link(0, 0).unwrap();
    assert_eq!(branch.merge().unwrap(), 0);
    unfinished.rollback();
    assert!(parent.snapshot().unwrap().links().is_empty());
}
