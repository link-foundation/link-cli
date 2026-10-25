//! Finite abrupt exits exercise actual file framing, ownership and reopening.
//! They model a lost process, not loss of the OS page cache or device power.
#![cfg(not(target_arch = "wasm32"))]
use link_cli::engine::{
    DeltaLog, Engine, EngineOptions, GroupCommitOutcome, HistoryPolicy, IsolationLevel,
    RevisionEncoding, StateStructure,
};
use link_cli::{FileTransitionLog, LinkError, LinksStorage, TransitionLogStore};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

struct ExitLog {
    inner: FileTransitionLog,
    point: String,
    appends: usize,
}
impl TransitionLogStore for ExitLog {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.inner.read_log_entries()
    }
    fn append_log_entry(&mut self, record: &str) -> Result<(), LinkError> {
        if self.point == "before-append" && self.appends == 0 {
            std::process::exit(71);
        }
        self.inner.append_log_entry(record)?;
        self.appends += 1;
        if self.point == "after-append" && self.appends == 1 {
            std::process::exit(71);
        }
        Ok(())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        if self.point == "before-flush" {
            std::process::exit(71);
        }
        self.inner.flush_log()?;
        if self.point == "after-flush" {
            std::process::exit(71);
        }
        Ok(())
    }
}
fn options(revision_encoding: RevisionEncoding, state_structure: StateStructure) -> EngineOptions {
    EngineOptions {
        revision_encoding,
        state_structure,
        isolation: IsolationLevel::Snapshot,
        history: HistoryPolicy::All,
        require_durable_flush: true,
        ..Default::default()
    }
}
fn child_group<L: TransitionLogStore>(log: L, options: EngineOptions) {
    let db = Engine::<u32, _>::open(log, options).unwrap();
    let mut a = db.begin().unwrap();
    a.create_link(0, 0).unwrap();
    a.set_name(1, Some("first 雪".into())).unwrap();
    let mut b = db.begin().unwrap();
    b.create_link(1, 0).unwrap();
    b.set_name(2, Some("second".into())).unwrap();
    assert_eq!(
        db.commit_group(vec![a, b]).unwrap(),
        [
            GroupCommitOutcome::Committed(1),
            GroupCommitOutcome::Committed(2)
        ]
    );
    // Only the acknowledged control reaches this exit; skip all destructors.
    std::process::exit(0);
}
#[test]
#[ignore = "Executed by the bounded group-exit parent"]
fn group_exit_child() {
    let path = std::env::var("LINK_GROUP_EXIT_PATH").unwrap();
    let mut inner = FileTransitionLog::open_exclusive(path).unwrap();
    inner.set_sync_on_append(false);
    let log = ExitLog {
        inner,
        point: std::env::var("LINK_GROUP_EXIT_POINT").unwrap(),
        appends: 0,
    };
    let encoding = if std::env::var("LINK_GROUP_EXIT_ENCODING").unwrap() == "changes" {
        RevisionEncoding::Changes
    } else {
        RevisionEncoding::Snapshot
    };
    let structure = if std::env::var("LINK_GROUP_EXIT_STRUCTURE").unwrap() == "shared" {
        StateStructure::Shared
    } else {
        StateStructure::Flat
    };
    let options = options(encoding, structure);
    if std::env::var("LINK_GROUP_EXIT_DELTA").unwrap() == "true" {
        child_group(DeltaLog::new(log, 128).unwrap(), options);
    } else {
        child_group(log, options);
    }
}
fn verify<L: TransitionLogStore>(log: L, prefix: u64, following: bool, options: EngineOptions) {
    let expected = prefix + u64::from(!following);
    let db = Engine::<u32, _>::open(log, options).unwrap();
    let snapshot = db.snapshot().unwrap();
    assert_eq!(snapshot.revision, expected);
    assert_eq!(snapshot.rows().len(), expected as usize);
    if prefix > 0 {
        assert_eq!(
            db.snapshot_at(1).unwrap().get(1).unwrap().name.as_deref(),
            Some("first 雪")
        );
    }
    if prefix > 1 {
        assert_eq!(
            db.snapshot_at(2).unwrap().get(2).unwrap().name.as_deref(),
            Some("second")
        );
    }
    if following {
        let mut next = db.begin().unwrap();
        // The first accepted record already persists both reserved addresses.
        assert_eq!(
            next.create_link(0, 0).unwrap(),
            if prefix == 0 { 1 } else { 3 }
        );
        assert_eq!(next.commit().unwrap(), expected + 1);
    }
}
#[test]
fn every_abrupt_group_exit_recovers_its_exact_prefix_and_following_commit() {
    for encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
        for structure in [StateStructure::Flat, StateStructure::Shared] {
            for delta in [false, true] {
                for (point, expected) in [
                    ("before-append", 0),
                    ("after-append", 1),
                    ("before-flush", 2),
                    ("after-flush", 2),
                    ("acknowledged", 2),
                ] {
                    let directory = tempfile::tempdir().unwrap();
                    let path = directory.path().join("journal");
                    let mut child = Command::new(std::env::current_exe().unwrap())
                        .args(["--exact", "group_exit_child", "--ignored", "--nocapture"])
                        .env("LINK_GROUP_EXIT_PATH", &path)
                        .env("LINK_GROUP_EXIT_POINT", point)
                        .env("LINK_GROUP_EXIT_DELTA", delta.to_string())
                        .env(
                            "LINK_GROUP_EXIT_ENCODING",
                            if encoding == RevisionEncoding::Changes {
                                "changes"
                            } else {
                                "snapshot"
                            },
                        )
                        .env(
                            "LINK_GROUP_EXIT_STRUCTURE",
                            if structure == StateStructure::Shared {
                                "shared"
                            } else {
                                "flat"
                            },
                        )
                        .stdout(Stdio::null())
                        .stderr(Stdio::piped())
                        .spawn()
                        .unwrap();
                    let deadline = Instant::now() + Duration::from_secs(15);
                    loop {
                        if child.try_wait().unwrap().is_some() {
                            break;
                        }
                        if Instant::now() >= deadline {
                            child.kill().unwrap();
                            child.wait().unwrap();
                            panic!("finite group child expired");
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    let output = child.wait_with_output().unwrap();
                    assert_eq!(
                        output.status.code(),
                        Some(if point == "acknowledged" { 0 } else { 71 }),
                        "{point}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    for reopen in 0..2 {
                        let log = FileTransitionLog::open_exclusive(&path).unwrap();
                        let future = if reopen == 0 {
                            if encoding == RevisionEncoding::Snapshot {
                                RevisionEncoding::Changes
                            } else {
                                RevisionEncoding::Snapshot
                            }
                        } else {
                            encoding
                        };
                        if delta {
                            verify(
                                DeltaLog::new(log, 128).unwrap(),
                                expected,
                                reopen == 0,
                                options(future, structure),
                            );
                        } else {
                            verify(log, expected, reopen == 0, options(future, structure));
                        }
                    }
                }
            }
        }
    }
}
