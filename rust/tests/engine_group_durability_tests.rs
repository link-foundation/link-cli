//! Native file persistence and bounded publication gates for grouped sessions.
#![cfg(not(target_arch = "wasm32"))]
use link_cli::engine::replication::{ReplicatedLog, Synchronization};
use link_cli::engine::{
    DeltaLog, Engine, EngineOptions, GroupCommitOutcome, HistoryPolicy, IsolationLevel,
};
use link_cli::{FileTransitionLog, LinkError, LinksStorage, TransitionLogStore};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

struct CountedLog<L> {
    inner: L,
    flushes: Arc<AtomicUsize>,
}
impl<L: TransitionLogStore> TransitionLogStore for CountedLog<L> {
    fn supports_durable_flush(&self) -> bool {
        self.inner.supports_durable_flush()
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.inner.read_log_entries()
    }
    fn append_log_entry(&mut self, record: &str) -> Result<(), LinkError> {
        self.inner.append_log_entry(record)
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.flushes.fetch_add(1, Ordering::SeqCst);
        self.inner.flush_log()
    }
}

#[test]
fn buffered_durable_delta_replicas_reopen_every_independent_revision() {
    let dir = tempfile::tempdir().unwrap();
    let paths = [dir.path().join("primary"), dir.path().join("follower")];
    let counters = [Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))];
    let mut logs = paths.iter().zip(&counters).map(|(path, count)| {
        let mut inner = FileTransitionLog::open_exclusive(path).unwrap();
        inner.set_sync_on_append(false);
        CountedLog {
            inner,
            flushes: count.clone(),
        }
    });
    let replication = ReplicatedLog::new(
        logs.next().unwrap(),
        vec![logs.next().unwrap()],
        Synchronization::Synchronous {
            acknowledgements: 2,
        },
    )
    .unwrap();
    let options = EngineOptions {
        isolation: IsolationLevel::Snapshot,
        history: HistoryPolicy::All,
        require_durable_flush: true,
        ..Default::default()
    };
    let db =
        Engine::<u32, _>::open(DeltaLog::new(replication, 128).unwrap(), options.clone()).unwrap();
    let mut seed = db.begin().unwrap();
    for id in 1..=32 {
        seed.ensure_link_created(id).unwrap();
    }
    seed.commit().unwrap();
    drop(seed);
    for count in &counters {
        count.store(0, Ordering::SeqCst);
    }
    let mut a = db.begin().unwrap();
    a.set_name(1, Some("one 雪".into())).unwrap();
    let mut b = db.begin().unwrap();
    b.set_name(2, Some("two".into())).unwrap();
    assert_eq!(
        db.commit_group(vec![a, b]).unwrap(),
        [
            GroupCommitOutcome::Committed(2),
            GroupCommitOutcome::Committed(3)
        ]
    );
    for count in &counters {
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
    drop(db);
    assert_eq!(
        std::fs::read(&paths[0]).unwrap(),
        std::fs::read(&paths[1]).unwrap()
    );
    for path in &paths {
        let mut raw = FileTransitionLog::open_exclusive(path).unwrap();
        assert!(raw.read_log_entries().unwrap()[1].starts_with("delta1|"));
        let reopened =
            Engine::<u32, _>::open(DeltaLog::new(raw, 128).unwrap(), options.clone()).unwrap();
        assert_eq!(reopened.snapshot_at(1).unwrap().get(1).unwrap().name, None);
        assert_eq!(
            reopened
                .snapshot_at(2)
                .unwrap()
                .get(1)
                .unwrap()
                .name
                .as_deref(),
            Some("one 雪")
        );
        assert_eq!(reopened.snapshot_at(2).unwrap().get(2).unwrap().name, None);
        assert_eq!(
            reopened
                .snapshot_at(3)
                .unwrap()
                .get(2)
                .unwrap()
                .name
                .as_deref(),
            Some("two")
        );
        let mut next = reopened.begin().unwrap();
        assert_eq!(next.create_link(0, 0).unwrap(), 33);
        assert_eq!(next.commit().unwrap(), 4);
    }
}

struct GateLog {
    records: Vec<String>,
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl TransitionLogStore for GateLog {
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.records.clone())
    }
    fn append_log_entry(&mut self, record: &str) -> Result<(), LinkError> {
        self.records.push(record.into());
        Ok(())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.entered.send(()).unwrap();
        self.release
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| LinkError::Transaction("bounded test gate expired".into()))
    }
}

#[test]
fn readers_cannot_observe_intermediate_group_publication_before_flush_returns() {
    let (entered, at_flush) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let db = Engine::<u32, _>::open(
        GateLog {
            records: vec![],
            entered,
            release: wait,
        },
        EngineOptions {
            isolation: IsolationLevel::Snapshot,
            ..Default::default()
        },
    )
    .unwrap();
    let mut a = db.begin().unwrap();
    a.create_link(0, 0).unwrap();
    let mut b = db.begin().unwrap();
    b.create_link(1, 0).unwrap();
    let writer = db.clone();
    let worker = std::thread::spawn(move || writer.commit_group(vec![a, b]).unwrap());
    at_flush.recv_timeout(Duration::from_secs(5)).unwrap();
    let reader = db.clone();
    let (done, result) = mpsc::channel();
    let (started, start) = mpsc::channel();
    let observer = std::thread::spawn(move || {
        started.send(()).unwrap();
        done.send(reader.snapshot().unwrap()).unwrap();
    });
    start.recv_timeout(Duration::from_secs(5)).unwrap();
    let withheld = matches!(
        result.recv_timeout(Duration::from_millis(25)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    release.send(()).unwrap();
    let outcomes = worker.join().unwrap();
    let snapshot = result.recv_timeout(Duration::from_secs(5)).unwrap();
    observer.join().unwrap();
    assert!(withheld);
    assert_eq!(snapshot.revision, 2);
    assert_eq!(snapshot.rows().len(), 2);
    assert_eq!(
        outcomes,
        [
            GroupCommitOutcome::Committed(1),
            GroupCommitOutcome::Committed(2)
        ]
    );
}
