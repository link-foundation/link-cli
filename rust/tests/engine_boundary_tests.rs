use link_cli::engine::cluster::{AtomicJournalStore, ClusterEngine, JournalVersion};
use link_cli::engine::{Engine, EngineOptions, IsolationLevel, MemoryLog};
use link_cli::{LinkError, LinksStorage, TransitionLogStore};

fn record(text: &str) -> String {
    // Independent checksum fixture: deliberately change fields, retain a valid hash.
    let hash = text.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    format!("{text}|{hash:016x}")
}

// A read-only adapter deliberately omits the optional durability capability.
struct ReadOnlyCoordinator;
impl AtomicJournalStore for ReadOnlyCoordinator {
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        Ok(vec![JournalVersion::default(); keys.len()])
    }
    fn compare_exchange(
        &mut self,
        _keys: &[String],
        _expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        if updates.iter().any(Option::is_some) {
            return Err(LinkError::Transaction("coordinator is read only".into()));
        }
        Ok(true)
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn coordinator_default_capability_cannot_satisfy_a_durable_requirement() {
    assert!(!ReadOnlyCoordinator.supports_durable_flush());
    assert!(ClusterEngine::<u64, _>::new(
        ReadOnlyCoordinator,
        EngineOptions {
            require_durable_flush: true,
            ..Default::default()
        }
    )
    .is_err());
    let cluster =
        ClusterEngine::<u64, _>::new(ReadOnlyCoordinator, EngineOptions::default()).unwrap();
    let mut tx = cluster.begin(&["a"]).unwrap();
    assert_eq!(tx.shard("a").unwrap().links_count(), 0);
    tx.commit().unwrap();
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn individually_valid_records_must_form_a_contiguous_monotonic_journal() {
    for entries in [
        vec![record("engine1|2|1|")],
        vec![record("engine1|1|1|"), record("engine1|1|1|")],
        vec![record("engine1|1|1|"), record("engine1|3|1|")],
        vec![record("engine1|1|100|"), record("engine1|2|99|")],
    ] {
        assert!(Engine::<u64, _>::open(MemoryLog { entries }, EngineOptions::default()).is_err());
    }
    let db = Engine::<u64, _>::open(
        MemoryLog {
            entries: vec![record("engine1|1|100|"), record("engine1|2|100|")],
        },
        EngineOptions::default(),
    )
    .unwrap();
    assert_eq!(db.snapshot_at(2).unwrap().revision, 2);
    assert_eq!(db.begin().unwrap().create_link(0, 0).unwrap(), 100);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checksum_valid_empty_fields_are_rejected() {
    for text in [
        "engine1||1|",
        "engine1|1||",
        "engine1|1|2|,0,0,-",
        "engine1|1|2|1,,0,-",
        "engine1|1|2|1,0,,-",
    ] {
        assert!(
            Engine::<u64, _>::open(
                MemoryLog {
                    entries: vec![record(text)]
                },
                EngineOptions::default()
            )
            .is_err(),
            "{text}"
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn saved_merge_base_must_match_rows_and_allocator_as_well_as_revision() {
    let db = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let branch_log = MemoryLog {
        entries: vec![record("engine1|1|2|1,0,0,-")],
    };
    for text in ["engine1|1|2|1,1,0,-", "engine1|1|3|1,0,0,-"] {
        assert!(db
            .resume_branch(branch_log.clone(), EngineOptions::default(), &record(text))
            .is_err());
    }
    let branch = db
        .resume_branch(
            branch_log,
            EngineOptions::default(),
            &record("engine1|1|2|1,0,0,-"),
        )
        .unwrap();
    assert_eq!(branch.engine().snapshot().unwrap().links().len(), 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn failed_log_operation_fences_existing_and_new_sessions_until_reopen() {
    let db = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut pending = db.begin().unwrap();
    pending.create_link(0, 0).unwrap();
    // A caller's diagnostic/catch-up operation could fail after writing its log.
    let result = db.with_log(|log| {
        log.append_log_entry(&record("engine1|1|2|1,0,0,-"))?;
        Err::<(), _>(LinkError::Transaction(
            "injected lost diagnostic reply".into(),
        ))
    });
    assert!(result.unwrap_err().to_string().contains("diagnostic reply"));
    assert!(pending.commit().is_err());
    assert!(db.begin().is_err());
    assert!(db.snapshot().is_err());
    assert!(db.with_log(|_| Ok(())).is_err());
    drop(pending);
    let log = db.into_log().ok().unwrap();
    let reopened = Engine::<u64, _>::open(log, EngineOptions::default()).unwrap();
    assert_eq!(reopened.snapshot().unwrap().links().len(), 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn u128_allocator_exhaustion_is_bounded_and_never_wraps_to_zero() {
    let db = Engine::<u128, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = db.begin().unwrap();
    assert!(tx.ensure_link_created(u128::MAX).is_err());
    assert!(tx.all_links().is_empty());
    assert_eq!(
        tx.create_link(0, 0).unwrap(),
        1,
        "failed explicit allocation cannot advance the counter"
    );
    tx.ensure_link_created(u128::MAX - 1).unwrap();
    assert!(tx.create_link(0, 0).is_err());
    assert!(tx.create_link(0, 0).is_err());
    assert!(tx.get_link(0).is_none());
    tx.commit().unwrap();
    drop(tx);
    let log = db.into_log().ok().unwrap();
    let reopened = Engine::<u128, _>::open(log, EngineOptions::default()).unwrap();
    let mut next = reopened.begin().unwrap();
    assert_eq!(next.links_count(), 2);
    assert!(next.create_link(0, 0).is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn closed_read_committed_sessions_keep_their_base_after_rollback() {
    let db = Engine::<u64, _>::open(
        MemoryLog::default(),
        EngineOptions {
            isolation: IsolationLevel::ReadCommitted,
            ..Default::default()
        },
    )
    .unwrap();
    let mut seed = db.begin().unwrap();
    seed.create_link(0, 0).unwrap();
    seed.commit().unwrap();
    let mut reader = db.begin().unwrap();
    let mut writer = db.begin().unwrap();
    writer.set_name(1, Some("new".into())).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.get_name(1).as_deref(), Some("new"));
    assert_eq!(reader.all_links().len(), 1);
    reader.rollback();
    assert_eq!(reader.get_name(1), None);
    let mut deleted = db.begin().unwrap();
    deleted.delete_link(1).unwrap();
    deleted.commit().unwrap();
    assert!(reader.get_link(1).is_some());
    assert_eq!(reader.all_links().len(), 1);
    assert!(db.snapshot().unwrap().links().is_empty());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg(target_os = "linux")]
#[cfg(not(target_arch = "wasm32"))]
fn unavailable_diagnostic_output_cannot_fail_an_acknowledged_commit_or_catchup() {
    let directory = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for mode in ["commit", "replication"] {
        let path = directory.path().join(mode);
        // A bounded ENOSPC fault. Rust deliberately ignores EBADF on stderr,
        // so a read-only file handle would not exercise the failure.
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "trace_failure_child", "--ignored", "--nocapture"])
            .env("LINK_TRACE_FAILURE_PATH", &path)
            .env("LINK_TRACE_FAILURE_MODE", mode)
            .stderr(
                std::fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/full")
                    .unwrap(),
            )
            .output()
            .unwrap();
        let recovered = Engine::<u64, _>::open(
            link_cli::FileTransitionLog::open_exclusive(path).unwrap(),
            EngineOptions::default(),
        )
        .unwrap();
        if !child.status.success() {
            failures.push(format!(
                "{mode}: child {} after persisting {} rows; stdout={}",
                child.status,
                recovered.snapshot().unwrap().links().len(),
                String::from_utf8_lossy(&child.stdout)
            ));
            continue;
        }
        assert_eq!(recovered.snapshot().unwrap().links().len(), 2);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[cfg(not(target_arch = "wasm32"))]
struct UnavailableReplica;
#[cfg(not(target_arch = "wasm32"))]
impl TransitionLogStore for UnavailableReplica {
    fn append_log_entry(&mut self, _entry: &str) -> Result<(), LinkError> {
        Err(LinkError::Transaction("unavailable replica".into()))
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Err(LinkError::Transaction("unavailable replica".into()))
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        Err(LinkError::Transaction("unavailable replica".into()))
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[ignore = "child process entry point"]
#[cfg(not(target_arch = "wasm32"))]
fn trace_failure_child() {
    use link_cli::engine::replication::{ReplicatedLog, Synchronization};
    let Some(path) = std::env::var_os("LINK_TRACE_FAILURE_PATH") else {
        return;
    };
    let mode = std::env::var("LINK_TRACE_FAILURE_MODE").unwrap();
    let mut log = ReplicatedLog::new(
        link_cli::FileTransitionLog::open_exclusive(path).unwrap(),
        vec![UnavailableReplica],
        Synchronization::Asynchronous,
    )
    .unwrap();
    log.set_trace(mode == "replication");
    let db = Engine::<u64, _>::open(
        log,
        EngineOptions {
            trace: mode == "commit",
            ..Default::default()
        },
    )
    .unwrap();
    for _ in 0..2 {
        let mut tx = db.begin().unwrap();
        tx.create_link(0, 0).unwrap();
        tx.commit().unwrap();
        assert_eq!(db.with_log(ReplicatedLog::synchronize).unwrap(), 1);
    }
}
