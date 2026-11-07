use link_cli::engine::{
    DeltaLog, Engine, EngineOptions, HistoryPolicy, MemoryLog, RevisionEncoding, StateStructure,
};
use link_cli::{LinkError, LinksStorage, TransitionLogStore};

fn vectors() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../docs/protocol/engine-checkpoint-vectors.json"
    ))
    .unwrap()
}
fn strings(value: &serde_json::Value) -> Vec<String> {
    serde_json::from_value(value.clone()).unwrap()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn physical_delta_accepts_independent_initial_checkpoint_vectors() {
    let vectors = vectors();
    for value in vectors["valid"].as_array().unwrap() {
        let entries = strings(value);
        let mut log = DeltaLog::new(
            MemoryLog {
                entries: entries.clone(),
            },
            128,
        )
        .unwrap();
        assert_eq!(log.read_log_entries().unwrap(), entries);
        let mut appended = DeltaLog::new(MemoryLog::default(), 128).unwrap();
        for record in &entries {
            appended.append_log_entry(record).unwrap();
        }
        assert_eq!(appended.read_log_entries().unwrap(), entries);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checkpoint_compression_preserves_exact_bytes_and_change_boundaries() {
    let fixture = &vectors()["physicalDelta"];
    let logical = strings(&fixture["logical"]);
    let physical = strings(&fixture["physical"]);
    let mut log = DeltaLog::new(MemoryLog::default(), 128).unwrap();
    for record in &logical {
        log.append_log_entry(record).unwrap();
    }
    assert_eq!(log.read_log_entries().unwrap(), logical);
    assert_eq!(log.into_inner().entries, physical);
    let mut reopened = DeltaLog::new(MemoryLog { entries: physical }, 1).unwrap();
    assert_eq!(reopened.read_log_entries().unwrap(), logical);
    let zero = strings(&fixture["zeroFollow"]);
    let mut zero_log = DeltaLog::new(
        MemoryLog {
            entries: zero.clone(),
        },
        128,
    )
    .unwrap();
    assert_eq!(zero_log.read_log_entries().unwrap(), zero);
    for value in fixture["invalid"].as_array().unwrap() {
        let records = strings(value);
        assert!(
            DeltaLog::new(
                MemoryLog {
                    entries: records.clone()
                },
                128
            )
            .unwrap()
            .read_log_entries()
            .is_err(),
            "accepted {value}"
        );
        let mut log = DeltaLog::new(MemoryLog::default(), 128).unwrap();
        for record in &records[..records.len() - 1] {
            log.append_log_entry(record).unwrap();
        }
        let prefix = log.read_log_entries().unwrap();
        assert!(log.append_log_entry(records.last().unwrap()).is_err());
        assert_eq!(log.read_log_entries().unwrap(), prefix);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checkpoints_compose_with_delta_history_state_encoding_and_immutable_pins() {
    for state_structure in [
        StateStructure::Flat,
        StateStructure::Shared,
        StateStructure::SortedRuns,
    ] {
        for revision_encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
            for history in [
                HistoryPolicy::None,
                HistoryPolicy::Last(2),
                HistoryPolicy::All,
            ] {
                let options = EngineOptions {
                    state_structure,
                    revision_encoding,
                    history,
                    ..Default::default()
                };
                let engine = Engine::<u32, _>::open(MemoryLog::default(), options.clone()).unwrap();
                for revision in 1..=6 {
                    let mut tx = engine.begin().unwrap();
                    if revision == 1 {
                        for id in 1..=32 {
                            tx.ensure_link_created(id).unwrap();
                        }
                        tx.ensure_link_created(40).unwrap();
                        tx.delete_link(40).unwrap();
                    }
                    tx.set_name(16, Some(format!("雪;|😀 {revision}"))).unwrap();
                    tx.commit().unwrap();
                }
                let exported = engine.export_checkpoint().unwrap();
                let mut log = DeltaLog::new(MemoryLog::default(), 128).unwrap();
                for record in &exported {
                    log.append_log_entry(record).unwrap();
                }
                let copy = Engine::<u32, _>::open(log, options.clone()).unwrap();
                for revision in 0..=6 {
                    match (engine.snapshot_at(revision), copy.snapshot_at(revision)) {
                        (Ok(a), Ok(b)) => assert_eq!(a.rows(), b.rows()),
                        (Err(_), Err(_)) => {}
                        _ => panic!("history changed at {revision}"),
                    }
                }
                let pinned = copy.snapshot().unwrap();
                {
                    let mut tx = copy.begin().unwrap();
                    assert_eq!(tx.create_link(1, 0).unwrap(), 41);
                    tx.set_name(16, Some("after checkpoint".into())).unwrap();
                    tx.commit().unwrap();
                }
                assert_eq!(pinned.get(16).unwrap().name.as_deref(), Some("雪;|😀 6"));
                assert!(pinned.get(41).is_none());
                let mut log = copy.into_log().ok().unwrap();
                let logical = log.read_log_entries().unwrap();
                let physical = log.into_inner();
                assert_eq!(physical.entries[0], exported[0]);
                if revision_encoding == RevisionEncoding::Snapshot {
                    assert!(physical.entries.last().unwrap().starts_with("delta1|"));
                } else {
                    assert!(physical.entries.last().unwrap().starts_with("engine2|"));
                }
                let reopened =
                    Engine::<u32, _>::open(DeltaLog::new(physical, 1).unwrap(), options).unwrap();
                assert_eq!(
                    reopened.with_log(|log| log.read_log_entries()).unwrap(),
                    logical
                );
                assert_eq!(reopened.snapshot().unwrap().revision, 7);
                assert!(reopened.snapshot().unwrap().get(41).is_some());
            }
        }
    }
}

struct FaultLog {
    raw: MemoryLog,
    mode: &'static str,
    calls: usize,
}
impl TransitionLogStore for FaultLog {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.calls += 1;
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checkpoint_and_following_delta_uncertainty_fence_until_actual_prefix_reopen() {
    let records = strings(&vectors()["physicalDelta"]["logical"]);
    for prefix in 0..=1 {
        for mode in ["append-before", "append-after", "flush"] {
            let mut log = DeltaLog::new(
                FaultLog {
                    raw: MemoryLog {
                        entries: records[..prefix].to_vec(),
                    },
                    mode,
                    calls: 0,
                },
                128,
            )
            .unwrap();
            assert!(log.supports_durable_flush());
            if mode == "flush" {
                log.append_log_entry(&records[prefix]).unwrap();
                assert!(log.flush_log().is_err());
            } else {
                assert!(log.append_log_entry(&records[prefix]).is_err());
            }
            assert!(log
                .read_log_entries()
                .unwrap_err()
                .to_string()
                .contains("indeterminate"));
            assert!(log
                .append_log_entry(&records[prefix])
                .unwrap_err()
                .to_string()
                .contains("indeterminate"));
            assert!(log
                .flush_log()
                .unwrap_err()
                .to_string()
                .contains("indeterminate"));
            let inner = log.into_inner();
            assert_eq!(inner.calls, if mode == "flush" { 3 } else { 2 });
            let mut reopened = DeltaLog::new(inner.raw, 128).unwrap();
            let expected = prefix + usize::from(mode != "append-before");
            assert_eq!(reopened.read_log_entries().unwrap(), records[..expected]);
            for record in &records[expected..] {
                reopened.append_log_entry(record).unwrap();
            }
            assert_eq!(reopened.read_log_entries().unwrap(), records);
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checkpoint_replication_accepts_both_physical_composition_orders() {
    use link_cli::engine::replication::{ReplicatedLog, Synchronization};
    let logical = strings(&vectors()["physicalDelta"]["logical"]);
    let policy = Synchronization::Synchronous {
        acknowledgements: 2,
    };
    let replica =
        ReplicatedLog::new(MemoryLog::default(), vec![MemoryLog::default()], policy).unwrap();
    let mut outside = DeltaLog::new(replica, 128).unwrap();
    for record in &logical {
        outside.append_log_entry(record).unwrap();
        outside.flush_log().unwrap();
    }
    assert_eq!(outside.read_log_entries().unwrap(), logical);
    let (primary, followers) = outside.into_inner().into_parts();
    assert_eq!(primary.entries, followers[0].entries);
    assert!(primary.entries[1].starts_with("delta1|"));
    let mut inside = ReplicatedLog::new(
        DeltaLog::new(primary, 1).unwrap(),
        vec![DeltaLog::new(followers.into_iter().next().unwrap(), 128).unwrap()],
        policy,
    )
    .unwrap();
    // Reopen existing physical streams, then use different policies on new copies.
    assert_eq!(inside.read_log_entries().unwrap(), logical);
    assert_eq!(inside.synchronize().unwrap(), 2);
    let mut independent = ReplicatedLog::new(
        DeltaLog::new(MemoryLog::default(), 1).unwrap(),
        vec![DeltaLog::new(MemoryLog::default(), 128).unwrap()],
        policy,
    )
    .unwrap();
    for record in &logical {
        independent.append_log_entry(record).unwrap();
        independent.flush_log().unwrap();
    }
    assert_eq!(independent.synchronize().unwrap(), 2);
    let (mut primary, mut followers) = independent.into_parts();
    assert_eq!(primary.read_log_entries().unwrap(), logical);
    assert_eq!(followers[0].read_log_entries().unwrap(), logical);
    assert_ne!(
        primary.into_inner().entries,
        followers.remove(0).into_inner().entries
    );
}
