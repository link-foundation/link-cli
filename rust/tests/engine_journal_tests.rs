use link_cli::engine::replication::{ReplicatedLog, Synchronization};
use link_cli::engine::{
    Constraint, Engine, EngineOptions, HistoryPolicy, IsolationLevel, MemoryLog,
};
use link_cli::{FileTransitionLog, LinkError, LinksStorage, TransitionLogStore};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct FaultLog {
    records: Arc<Mutex<Vec<String>>>,
    fail: Arc<std::sync::atomic::AtomicBool>,
}
impl TransitionLogStore for FaultLog {
    fn append_log_entry(&mut self, text: &str) -> Result<(), LinkError> {
        self.records.lock().unwrap().push(text.into());
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.records.lock().unwrap().clone())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            Err(LinkError::Transaction(
                "injected partition/flush failure".into(),
            ))
        } else {
            Ok(())
        }
    }
}

#[test]
fn unknown_commit_outcome_requires_reopen() {
    let log = FaultLog::default();
    let db = Engine::<u64, _>::open(log.clone(), EngineOptions::default()).unwrap();
    let mut tx = db.begin().unwrap();
    tx.create_link(0, 0).unwrap();
    log.fail.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(tx
        .commit()
        .unwrap_err()
        .to_string()
        .contains("indeterminate"));
    assert!(db.begin().is_err());
    assert!(db.snapshot().is_err());
    assert!(
        tx.all_links().is_empty(),
        "failed session must discard its write set"
    );
    log.fail.store(false, std::sync::atomic::Ordering::SeqCst);
    let reopened = Engine::<u64, _>::open(log, EngineOptions::default()).unwrap();
    assert_eq!(reopened.snapshot().unwrap().links().len(), 1);
}

#[test]
fn unicode_names_history_deletion_and_replay() {
    let db = Engine::<u64, _>::open(
        MemoryLog::default(),
        EngineOptions {
            history: HistoryPolicy::All,
            ..Default::default()
        },
    )
    .unwrap();
    let mut tx = db.begin().unwrap();
    let id = tx.create_link(0, 0).unwrap();
    tx.set_name(id, Some("雪;|😀\n\0".into())).unwrap();
    tx.commit().unwrap();
    drop(tx);
    let mut tx = db.begin().unwrap();
    tx.delete_link(id).unwrap();
    tx.commit().unwrap();
    drop(tx);
    let log = db.into_log().ok().unwrap();
    let reopened = Engine::<u64, _>::open(
        log.clone(),
        EngineOptions {
            history: HistoryPolicy::All,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        reopened
            .snapshot_at(1)
            .unwrap()
            .get(id)
            .unwrap()
            .name
            .as_deref(),
        Some("雪;|😀\n\0")
    );
    assert!(reopened.snapshot_at(0).unwrap().links().is_empty());
    assert!(reopened.snapshot().unwrap().links().is_empty());
    let mut corrupt = log;
    corrupt.entries[0].push('0');
    assert!(Engine::<u64, _>::open(corrupt, EngineOptions::default()).is_err());
}

#[test]
fn constraints_check_the_entire_atomic_write_set() {
    for constraint in [
        Constraint::References,
        Constraint::UniqueNames,
        Constraint::UniquePairs,
    ] {
        let db = Engine::<u64, _>::open(
            MemoryLog::default(),
            EngineOptions {
                constraints: vec![constraint],
                ..Default::default()
            },
        )
        .unwrap();
        let mut tx = db.begin().unwrap();
        let a = tx.create_link(0, 0).unwrap();
        let b = tx.create_link(0, 0).unwrap();
        match constraint {
            Constraint::References => {
                tx.update_link(a, 999, 0).unwrap();
            }
            Constraint::UniqueNames => {
                tx.set_name(a, Some("same".into())).unwrap();
                tx.set_name(b, Some("same".into())).unwrap();
            }
            Constraint::UniquePairs => {}
        }
        assert!(tx.commit().is_err());
        assert!(db.snapshot().unwrap().links().is_empty());
    }
}

#[test]
fn asynchronous_replication_catches_up_idempotently() {
    let replica = FaultLog::default();
    let log = ReplicatedLog::new(
        MemoryLog::default(),
        vec![replica.clone()],
        Synchronization::Asynchronous,
    )
    .unwrap();
    let db = Engine::<u64, _>::open(log, EngineOptions::default()).unwrap();
    let mut tx = db.begin().unwrap();
    tx.create_link(0, 0).unwrap();
    tx.commit().unwrap();
    assert!(replica.records.lock().unwrap().is_empty());
    assert_eq!(db.with_log(ReplicatedLog::synchronize).unwrap(), 2);
    assert_eq!(db.with_log(ReplicatedLog::synchronize).unwrap(), 2);
    assert_eq!(replica.records.lock().unwrap().len(), 1);
    let follower = Engine::<u64, _>::open(replica, EngineOptions::default()).unwrap();
    assert_eq!(follower.snapshot().unwrap().links().len(), 1);
}

#[test]
fn synchronous_replication_reports_partition_and_does_not_diverge() {
    let replica = FaultLog::default();
    replica
        .fail
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let log = ReplicatedLog::new(
        MemoryLog::default(),
        vec![replica.clone()],
        Synchronization::Synchronous {
            acknowledgements: 2,
        },
    )
    .unwrap();
    let db = Engine::<u64, _>::open(log, EngineOptions::default()).unwrap();
    let mut tx = db.begin().unwrap();
    tx.create_link(0, 0).unwrap();
    assert!(tx.commit().is_err());
    assert!(db.begin().is_err());
    drop(tx);
    let mut log = db.into_log().ok().unwrap();
    replica
        .fail
        .store(false, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(log.synchronize().unwrap(), 2);
    assert_eq!(replica.records.lock().unwrap().len(), 1);
    replica.records.lock().unwrap()[0] = "diverged".into();
    assert_eq!(log.synchronize().unwrap(), 1);
}

#[test]
fn durable_requirement_rejects_volatile_logs_and_invalid_policies() {
    assert!(Engine::<u64, _>::open(
        MemoryLog::default(),
        EngineOptions {
            require_durable_flush: true,
            ..Default::default()
        }
    )
    .is_err());
    assert!(Engine::<u64, _>::open(
        MemoryLog::default(),
        EngineOptions {
            history: HistoryPolicy::Last(0),
            ..Default::default()
        }
    )
    .is_err());
    for acknowledgements in [0, 2] {
        assert!(ReplicatedLog::<_, MemoryLog>::new(
            MemoryLog::default(),
            vec![],
            Synchronization::Synchronous { acknowledgements }
        )
        .is_err());
    }
}

#[test]
fn concurrent_nonconflicting_snapshot_writers_preserve_every_row() {
    let db = Engine::<u64, _>::open(
        MemoryLog::default(),
        EngineOptions {
            isolation: IsolationLevel::Snapshot,
            ..Default::default()
        },
    )
    .unwrap();
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let db = db.clone();
            std::thread::spawn(move || {
                for _ in 0..16 {
                    let mut tx = db.begin().unwrap();
                    tx.create_link(0, 0).unwrap();
                    tx.commit().unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(db.snapshot().unwrap().links().len(), 64);
}

#[test]
fn exclusive_file_log_recovers_after_exit_without_destructors() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_exit_child", "--ignored"])
        .env("LINK_ENGINE_CRASH_PATH", &path)
        .status()
        .unwrap();
    assert!(status.success());
    let log = FileTransitionLog::open_exclusive(&path).unwrap();
    assert!(FileTransitionLog::open_exclusive(&path).is_err());
    let db = Engine::<u64, _>::open(
        log,
        EngineOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(db.snapshot().unwrap().links().len(), 1);
}

#[test]
#[ignore = "child process entry point"]
fn process_exit_child() {
    let Some(path) = std::env::var_os("LINK_ENGINE_CRASH_PATH") else {
        return;
    };
    let db = Engine::<u64, _>::open(
        FileTransitionLog::open_exclusive(path).unwrap(),
        EngineOptions::default(),
    )
    .unwrap();
    let mut tx = db.begin().unwrap();
    tx.create_link(0, 0).unwrap();
    tx.commit().unwrap();
    let mut uncommitted = db.begin().unwrap();
    uncommitted.create_link(0, 0).unwrap();
    std::process::exit(0);
}
