//! Native-only bounded proof that commits cannot cross checkpoint publication.
use link_cli::engine::{Engine, EngineOptions, MemoryLog};
use link_cli::{LinkError, LinksStorage, ReplaceableTransitionLogStore, TransitionLogStore};
use std::{sync::mpsc, thread, time::Duration};
struct BlockingLog {
    inner: MemoryLog,
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl TransitionLogStore for BlockingLog {
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.inner.flush_log()
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.inner.read_log_entries()
    }
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        self.inner.append_log_entry(entry)
    }
}
impl ReplaceableTransitionLogStore for BlockingLog {
    fn replace_log_entries(&mut self, entries: &[String]) -> Result<bool, LinkError> {
        self.entered.send(()).unwrap();
        self.release.recv_timeout(Duration::from_secs(10)).unwrap();
        let changed = self.inner.entries != entries;
        self.inner.entries = entries.to_vec();
        Ok(changed)
    }
}
#[test]
fn compaction_holds_the_commit_mutex_through_publication() {
    let (entered, wait_entered) = mpsc::channel();
    let (release, wait_release) = mpsc::channel();
    let engine = Engine::<u32, _>::open(
        BlockingLog {
            inner: MemoryLog::default(),
            entered,
            release: wait_release,
        },
        EngineOptions::default(),
    )
    .unwrap();
    let mut tx = engine.begin().unwrap();
    tx.create_link(0, 0).unwrap();
    let compactor = engine.clone();
    let compact = thread::spawn(move || compactor.compact_journal());
    wait_entered.recv_timeout(Duration::from_secs(10)).unwrap();
    let (started, wait_started) = mpsc::channel();
    let (finished, wait_finished) = mpsc::channel();
    let commit = thread::spawn(move || {
        started.send(()).unwrap();
        finished.send(tx.commit()).unwrap();
    });
    wait_started.recv_timeout(Duration::from_secs(10)).unwrap();
    let blocked = matches!(
        wait_finished.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    // Always release and join both children before asserting the blocked result.
    release.send(()).unwrap();
    assert!(compact.join().unwrap().unwrap());
    let revision = if blocked {
        Some(
            wait_finished
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap(),
        )
    } else {
        None
    };
    commit.join().unwrap();
    assert!(blocked, "a commit crossed an unpublished checkpoint");
    assert_eq!(revision, Some(1));
    let records = engine.with_log(|log| log.read_log_entries()).unwrap();
    assert_eq!(records.len(), 2);
    assert!(records[0].starts_with("engine3|0|"));
    let restored =
        Engine::<u32, _>::open(MemoryLog { entries: records }, EngineOptions::default()).unwrap();
    assert_eq!(restored.snapshot().unwrap().revision, 1);
    assert_eq!(restored.snapshot().unwrap().rows().len(), 1);
}
