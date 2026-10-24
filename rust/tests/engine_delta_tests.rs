use link_cli::engine::{DeltaLog, Engine, EngineOptions, HistoryPolicy, MemoryLog};
use link_cli::{FileTransitionLog, LinkError, LinksStorage, TransitionLogStore};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[test]
fn portable_delta_vectors_replay_and_reject_complete_corruption() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/engine-delta-vectors.json"
    ))
    .unwrap();
    let strings = |value: &serde_json::Value| {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|text| text.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    for sequence in vectors["validSequences"].as_array().unwrap() {
        let mut log = DeltaLog::new(
            MemoryLog {
                entries: strings(&sequence["physical"]),
            },
            128,
        )
        .unwrap();
        assert_eq!(
            log.read_log_entries().unwrap(),
            strings(&sequence["logical"])
        );
    }
    for sequence in vectors["invalidSequences"].as_array().unwrap() {
        let mut log = DeltaLog::new(
            MemoryLog {
                entries: strings(&sequence["records"]),
            },
            128,
        )
        .unwrap();
        assert!(log.read_log_entries().is_err(), "{}", sequence["name"]);
    }
}

#[test]
fn delta_journal_replays_exact_revisions_and_periodic_checkpoints() {
    let log = DeltaLog::new(MemoryLog::default(), 3).unwrap();
    let engine = Engine::<u32, _>::open(
        log,
        EngineOptions {
            history: HistoryPolicy::All,
            ..Default::default()
        },
    )
    .unwrap();
    {
        let mut tx = engine.begin().unwrap();
        for id in 1..=256 {
            tx.ensure_link_created(id).unwrap();
        }
        tx.commit().unwrap();
    }
    for name in ["first", "雪;|😀", "checkpoint"] {
        let mut tx = engine.begin().unwrap();
        tx.set_name(128, Some(name.into())).unwrap();
        tx.commit().unwrap();
    }
    let mut log = engine.into_log().ok().unwrap();
    let logical = log.read_log_entries().unwrap();
    let physical = log.into_inner();
    assert!(physical.entries[0].starts_with("engine1|"));
    assert!(physical.entries[1].starts_with("delta1|"));
    assert!(physical.entries[2].starts_with("delta1|"));
    assert!(physical.entries[3].starts_with("engine1|"));
    assert!(physical.entries[1].len() < logical[1].len() / 8);
    assert!(Engine::<u32, _>::open(physical.clone(), EngineOptions::default()).is_err());
    let reopened = Engine::<u32, _>::open(
        DeltaLog::new(physical, 1).unwrap(),
        EngineOptions {
            history: HistoryPolicy::All,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        reopened
            .snapshot_at(2)
            .unwrap()
            .get(128)
            .unwrap()
            .name
            .as_deref(),
        Some("first")
    );
    assert_eq!(
        reopened
            .snapshot_at(3)
            .unwrap()
            .get(128)
            .unwrap()
            .name
            .as_deref(),
        Some("雪;|😀")
    );
    assert_eq!(
        reopened
            .snapshot()
            .unwrap()
            .get(128)
            .unwrap()
            .name
            .as_deref(),
        Some("checkpoint")
    );
    assert_eq!(
        reopened.with_log(|log| log.read_log_entries()).unwrap(),
        logical
    );
}

#[test]
fn delta_configuration_and_full_record_fallback_are_explicit() {
    assert!(DeltaLog::new(MemoryLog::default(), 0).is_err());
    let log = DeltaLog::new(MemoryLog::default(), 1).unwrap();
    assert!(!log.supports_durable_flush());
    let engine = Engine::<u32, _>::open(log, EngineOptions::default()).unwrap();
    for _ in 0..2 {
        let mut tx = engine.begin().unwrap();
        tx.create_link(0, 0).unwrap();
        tx.commit().unwrap();
    }
    let physical = engine.into_log().ok().unwrap().into_inner();
    assert!(physical
        .entries
        .iter()
        .all(|text| text.starts_with("engine1|")));
}

#[derive(Default)]
struct FaultLog {
    raw: MemoryLog,
    mode: &'static str,
    calls: usize,
    read_fail: Arc<AtomicBool>,
}
impl TransitionLogStore for FaultLog {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.calls += 1;
        if self.mode == "read" || self.read_fail.load(Ordering::SeqCst) {
            return Err(LinkError::Transaction("read unavailable".into()));
        }
        self.raw.read_log_entries()
    }
    fn append_log_entry(&mut self, record: &str) -> Result<(), LinkError> {
        self.calls += 1;
        if self.mode == "append-before" {
            return Err(LinkError::Transaction("append before".into()));
        }
        self.raw.append_log_entry(record)?;
        if self.mode == "append-after" {
            return Err(LinkError::Transaction("append after".into()));
        }
        Ok(())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.calls += 1;
        if self.mode == "flush" {
            return Err(LinkError::Transaction("flush unavailable".into()));
        }
        Ok(())
    }
}

#[test]
fn uncertain_mutations_fence_all_operations_until_reopen() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/engine-delta-vectors.json"
    ))
    .unwrap();
    let record = vectors["validSequences"][0]["logical"][0].as_str().unwrap();
    for mode in ["append-before", "append-after", "flush"] {
        let mut log = DeltaLog::new(
            FaultLog {
                mode,
                ..Default::default()
            },
            128,
        )
        .unwrap();
        assert!(log.supports_durable_flush());
        if mode == "flush" {
            log.append_log_entry(record).unwrap();
            assert!(log.flush_log().is_err());
        } else {
            assert!(log.append_log_entry(record).is_err());
        }
        for result in [
            log.read_log_entries().map(|_| ()),
            log.append_log_entry(record),
            log.flush_log(),
        ] {
            assert!(result.unwrap_err().to_string().contains("indeterminate"));
        }
        let inner = log.into_inner();
        assert_eq!(inner.calls, if mode == "flush" { 3 } else { 2 });
        let mut fresh = DeltaLog::new(inner.raw, 128).unwrap();
        assert_eq!(
            fresh.read_log_entries().unwrap().len(),
            usize::from(mode != "append-before")
        );
    }
    let mut unavailable = DeltaLog::new(
        FaultLog {
            mode: "read",
            ..Default::default()
        },
        128,
    )
    .unwrap();
    assert!(unavailable
        .append_log_entry(record)
        .unwrap_err()
        .to_string()
        .contains("unavailable"));
    assert!(unavailable
        .read_log_entries()
        .unwrap_err()
        .to_string()
        .contains("unavailable"));
    assert_eq!(unavailable.into_inner().calls, 2);
}

#[test]
fn input_rejection_and_small_record_fallback_do_not_mutate_the_log() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/engine-delta-vectors.json"
    ))
    .unwrap();
    let mut log = DeltaLog::new(MemoryLog::default(), u32::MAX).unwrap();
    for value in vectors["invalidSequences"].as_array().unwrap() {
        let records = value["records"].as_array().unwrap();
        if records.len() == 1 {
            assert!(log.append_log_entry(records[0].as_str().unwrap()).is_err());
        }
    }
    assert!(log.read_log_entries().unwrap().is_empty());
    let small = vectors["validSequences"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    for record in small["logical"].as_array().unwrap() {
        log.append_log_entry(record.as_str().unwrap()).unwrap();
    }
    assert!(log
        .append_log_entry(small["logical"][1].as_str().unwrap())
        .is_err());
    assert_eq!(log.into_inner().entries.len(), 2);
}

#[test]
fn durable_replicas_store_identical_physical_deltas_and_reopen_with_history() {
    use link_cli::engine::replication::{ReplicatedLog, Synchronization};
    let dir = tempfile::tempdir().unwrap();
    let primary = dir.path().join("primary.log");
    let follower = dir.path().join("follower.log");
    let physical = ReplicatedLog::new(
        FileTransitionLog::open_exclusive(&primary).unwrap(),
        vec![FileTransitionLog::open_exclusive(&follower).unwrap()],
        Synchronization::Synchronous {
            acknowledgements: 2,
        },
    )
    .unwrap();
    let engine = Engine::<u32, _>::open(
        DeltaLog::new(physical, 128).unwrap(),
        EngineOptions {
            require_durable_flush: true,
            history: HistoryPolicy::All,
            ..Default::default()
        },
    )
    .unwrap();
    let mut seed = engine.begin().unwrap();
    for id in 1..=200 {
        seed.ensure_link_created(id).unwrap();
    }
    seed.commit().unwrap();
    drop(seed);
    let mut update = engine.begin().unwrap();
    update.set_name(100, Some("replicated".into())).unwrap();
    update.commit().unwrap();
    drop(update);
    drop(engine);
    assert_eq!(
        std::fs::read(&primary).unwrap(),
        std::fs::read(&follower).unwrap()
    );
    let mut raw = FileTransitionLog::open_exclusive(&follower).unwrap();
    assert!(raw.read_log_entries().unwrap()[1].starts_with("delta1|"));
    let recovered = Engine::<u32, _>::open(
        DeltaLog::new(raw, 128).unwrap(),
        EngineOptions {
            history: HistoryPolicy::All,
            require_durable_flush: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        recovered.snapshot_at(1).unwrap().get(100).unwrap().name,
        None
    );
    assert_eq!(
        recovered
            .snapshot()
            .unwrap()
            .get(100)
            .unwrap()
            .name
            .as_deref(),
        Some("replicated")
    );
}

#[test]
fn replicas_can_compress_independently_while_comparing_logical_prefixes() {
    use link_cli::engine::replication::{ReplicatedLog, Synchronization};
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/engine-delta-vectors.json"
    ))
    .unwrap();
    let logical = vectors["validSequences"][0]["logical"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let mut log = ReplicatedLog::new(
        DeltaLog::new(MemoryLog::default(), 1).unwrap(),
        vec![DeltaLog::new(MemoryLog::default(), 128).unwrap()],
        Synchronization::Synchronous {
            acknowledgements: 2,
        },
    )
    .unwrap();
    for record in &logical {
        log.append_log_entry(record).unwrap();
        log.flush_log().unwrap();
    }
    assert_eq!(log.synchronize().unwrap(), 2);
    let (mut primary, mut followers) = log.into_parts();
    assert_eq!(primary.read_log_entries().unwrap(), logical);
    assert_eq!(followers[0].read_log_entries().unwrap(), logical);
    assert_ne!(
        primary.into_inner().entries,
        followers.remove(0).into_inner().entries
    );
}

#[test]
fn failed_reread_requires_revalidation_before_append() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/engine-delta-vectors.json"
    ))
    .unwrap();
    let sequence = &vectors["validSequences"][0];
    let read_fail = Arc::new(AtomicBool::new(false));
    let inner = FaultLog {
        read_fail: read_fail.clone(),
        raw: MemoryLog {
            entries: sequence["physical"].as_array().unwrap()[..2]
                .iter()
                .map(|s| s.as_str().unwrap().into())
                .collect(),
        },
        ..Default::default()
    };
    let mut log = DeltaLog::new(inner, 128).unwrap();
    assert_eq!(log.read_log_entries().unwrap().len(), 2);
    read_fail.store(true, Ordering::SeqCst);
    assert!(log.read_log_entries().is_err());
    let next = sequence["logical"][2].as_str().unwrap();
    assert!(log.append_log_entry(next).is_err());
    read_fail.store(false, Ordering::SeqCst);
    log.append_log_entry(next).unwrap();
    let inner = log.into_inner();
    assert_eq!(inner.raw.entries.len(), 3);
    assert_eq!(inner.calls, 5);
}
