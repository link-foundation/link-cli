//! Independent revisions can share a flush without sharing a transaction.
use link_cli::engine::{
    ConcurrencyControl, Constraint, Engine, EngineOptions, GroupCommitOutcome, HistoryPolicy,
    IsolationLevel, MemoryLog, SerializableValidation,
};
use link_cli::{LinkError, LinksStorage, TransitionLogStore};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct LogState {
    entries: Vec<String>,
    flushes: usize,
}
#[derive(Clone, Default)]
struct ProbeLog(Arc<Mutex<LogState>>);
impl TransitionLogStore for ProbeLog {
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.0.lock().unwrap().entries.clone())
    }
    fn append_log_entry(&mut self, record: &str) -> Result<(), LinkError> {
        self.0.lock().unwrap().entries.push(record.into());
        Ok(())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.0.lock().unwrap().flushes += 1;
        Ok(())
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn intervening_aba_commits_remain_conflicts_for_group_members() {
    for (read_dependency, grouped) in [(false, false), (true, false), (false, true), (true, true)] {
        let log = ProbeLog::default();
        let db = Engine::<u32, _>::open(
            log.clone(),
            EngineOptions {
                isolation: if read_dependency {
                    IsolationLevel::Serializable
                } else {
                    IsolationLevel::Snapshot
                },
                serializable_validation: if read_dependency {
                    SerializableValidation::ReadSet
                } else {
                    SerializableValidation::Revision
                },
                ..Default::default()
            },
        )
        .unwrap();
        let mut seed = db.begin().unwrap();
        seed.ensure_link_created(1).unwrap();
        seed.commit().unwrap();
        let mut stale = db.begin().unwrap();
        if read_dependency {
            assert_eq!(stale.get_name(1), None);
            stale.create_link(0, 0).unwrap();
        } else {
            stale.set_name(1, Some("stale".into())).unwrap();
        }
        let mut ordinary = db.begin().unwrap();
        ordinary.set_name(1, Some("temporary".into())).unwrap();
        if grouped {
            assert_eq!(
                db.commit_group(vec![ordinary]).unwrap(),
                [GroupCommitOutcome::Committed(2)]
            );
        } else {
            assert_eq!(ordinary.commit().unwrap(), 2);
        }
        let mut ordinary = db.begin().unwrap();
        ordinary.set_name(1, None).unwrap();
        if grouped {
            assert_eq!(
                db.commit_group(vec![ordinary]).unwrap(),
                [GroupCommitOutcome::Committed(3)]
            );
        } else {
            assert_eq!(ordinary.commit().unwrap(), 3);
        }
        let mut valid = db.begin().unwrap();
        valid.create_link(0, 0).unwrap();
        let outcomes = db.commit_group(vec![stale, valid]).unwrap();
        assert!(
            matches!(&outcomes[0], GroupCommitOutcome::Aborted(message) if message.contains(if read_dependency { "read" } else { "written" }))
        );
        assert_eq!(outcomes[1], GroupCommitOutcome::Committed(4));
        assert_eq!(db.snapshot().unwrap().get(1).unwrap().name, None);
        assert_eq!(log.0.lock().unwrap().flushes, 4);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn synchronous_replication_flushes_each_copy_once_and_explicit_catchup_still_flushes() {
    use link_cli::engine::replication::{ReplicatedLog, Synchronization};
    let primary = ProbeLog::default();
    let follower = ProbeLog::default();
    let log = ReplicatedLog::new(
        primary.clone(),
        vec![follower.clone()],
        Synchronization::Synchronous {
            acknowledgements: 2,
        },
    )
    .unwrap();
    let db = Engine::<u32, _>::open(log, options()).unwrap();
    let mut a = db.begin().unwrap();
    a.create_link(0, 0).unwrap();
    let mut b = db.begin().unwrap();
    b.create_link(1, 0).unwrap();
    assert_eq!(
        db.commit_group(vec![a, b]).unwrap(),
        [
            GroupCommitOutcome::Committed(1),
            GroupCommitOutcome::Committed(2)
        ]
    );
    assert_eq!(primary.0.lock().unwrap().flushes, 1);
    assert_eq!(follower.0.lock().unwrap().flushes, 1);
    assert_eq!(
        primary.0.lock().unwrap().entries,
        follower.0.lock().unwrap().entries
    );
    assert_eq!(db.with_log(|log| log.synchronize()).unwrap(), 2);
    assert_eq!(primary.0.lock().unwrap().flushes, 2);
    assert_eq!(follower.0.lock().unwrap().flushes, 2);
    assert_eq!(follower.0.lock().unwrap().entries.len(), 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_sessions_keep_separate_revisions_with_one_flush() {
    let log = ProbeLog::default();
    let db = Engine::<u32, _>::open(
        log.clone(),
        EngineOptions {
            isolation: IsolationLevel::Snapshot,
            history: HistoryPolicy::All,
            ..Default::default()
        },
    )
    .unwrap();
    let mut first = db.begin().unwrap();
    let a = first.create_link(0, 0).unwrap();
    first.set_name(a, Some("first".into())).unwrap();
    let mut second = db.begin().unwrap();
    let b = second.create_link(a, 0).unwrap();
    second.set_name(b, Some("second".into())).unwrap();
    assert_eq!(db.snapshot().unwrap().revision, 0);
    assert_eq!(log.0.lock().unwrap().flushes, 0);
    assert_eq!(
        db.commit_group(vec![first, second]).unwrap(),
        [
            GroupCommitOutcome::Committed(1),
            GroupCommitOutcome::Committed(2)
        ]
    );
    assert_eq!(log.0.lock().unwrap().flushes, 1);
    assert_eq!(log.0.lock().unwrap().entries.len(), 2);
    assert_eq!(db.snapshot_at(1).unwrap().rows().len(), 1);
    assert_eq!(
        db.snapshot_at(2).unwrap().get(b).unwrap().name.as_deref(),
        Some("second")
    );
}

fn options() -> EngineOptions {
    EngineOptions {
        isolation: IsolationLevel::Snapshot,
        history: HistoryPolicy::All,
        ..Default::default()
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn entirely_aborted_writes_do_not_flush_and_following_group_can_commit() {
    let log = ProbeLog::default();
    let db = Engine::<u32, _>::open(
        log.clone(),
        EngineOptions {
            constraints: vec![Constraint::References],
            trace: true,
            ..options()
        },
    )
    .unwrap();
    let mut bad = db.begin().unwrap();
    bad.create_link(999, 0).unwrap();
    assert!(matches!(
        db.commit_group(vec![bad]).unwrap()[0],
        GroupCommitOutcome::Aborted(_)
    ));
    assert_eq!(log.0.lock().unwrap().flushes, 0);
    assert!(log.0.lock().unwrap().entries.is_empty());
    let mut good = db.begin().unwrap();
    assert_eq!(good.create_link(0, 0).unwrap(), 2);
    assert_eq!(
        db.commit_group(vec![good]).unwrap(),
        [GroupCommitOutcome::Committed(1)]
    );
    assert_eq!(log.0.lock().unwrap().flushes, 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn empty_read_only_closed_and_foreign_groups_do_no_journal_io() {
    let log = ProbeLog::default();
    let db = Engine::<u32, _>::open(log.clone(), options()).unwrap();
    assert!(db.commit_group(vec![]).unwrap().is_empty());
    let reader = db.begin().unwrap();
    let mut closed = db.begin().unwrap();
    closed.rollback();
    let outcomes = db.commit_group(vec![reader, closed]).unwrap();
    assert_eq!(outcomes[0], GroupCommitOutcome::Committed(0));
    assert!(
        matches!(&outcomes[1], GroupCommitOutcome::Aborted(message) if message.contains("closed"))
    );
    let other = Engine::<u32, _>::open(ProbeLog::default(), options()).unwrap();
    let mut local = db.begin().unwrap();
    local.create_link(0, 0).unwrap();
    let foreign = other.begin().unwrap();
    assert!(db.commit_group(vec![local, foreign]).is_err());
    assert_eq!(log.0.lock().unwrap().flushes, 0);
    assert!(log.0.lock().unwrap().entries.is_empty());
    let mut following = db.begin().unwrap();
    assert_eq!(following.create_link(0, 0).unwrap(), 2);
    assert_eq!(following.commit().unwrap(), 1);
    assert!(other.begin().is_ok());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn isolation_and_conflict_rules_match_individual_commits() {
    for isolation in [
        IsolationLevel::ReadCommitted,
        IsolationLevel::Snapshot,
        IsolationLevel::Serializable,
    ] {
        for validation in [
            SerializableValidation::Revision,
            SerializableValidation::ReadSet,
        ] {
            if isolation != IsolationLevel::Serializable
                && validation == SerializableValidation::ReadSet
            {
                continue;
            }
            let log = ProbeLog::default();
            let db = Engine::<u32, _>::open(
                log.clone(),
                EngineOptions {
                    isolation,
                    serializable_validation: validation,
                    ..options()
                },
            )
            .unwrap();
            let mut seed = db.begin().unwrap();
            seed.ensure_link_created(1).unwrap();
            seed.ensure_link_created(2).unwrap();
            seed.commit().unwrap();
            let mut a = db.begin().unwrap();
            let mut b = db.begin().unwrap();
            let mut same = db.begin().unwrap();
            a.set_name(1, Some("a".into())).unwrap();
            b.set_name(2, Some("b".into())).unwrap();
            same.set_name(1, Some("stale".into())).unwrap();
            let result = db.commit_group(vec![a, b, same]).unwrap();
            assert_eq!(result[0], GroupCommitOutcome::Committed(2));
            let disjoint = isolation != IsolationLevel::Serializable
                || validation == SerializableValidation::ReadSet;
            assert_eq!(
                matches!(result[1], GroupCommitOutcome::Committed(3)),
                disjoint
            );
            assert!(matches!(result[2], GroupCommitOutcome::Aborted(_)));
            assert_eq!(
                db.snapshot().unwrap().get(1).unwrap().name.as_deref(),
                Some("a")
            );
            assert_eq!(log.0.lock().unwrap().flushes, 2);
            let mut following = db.begin().unwrap();
            following.create_link(0, 0).unwrap();
            following.commit().unwrap();
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn read_dependencies_and_scans_abort_against_tentative_prefix() {
    for scan in [false, true] {
        let db = Engine::<u32, _>::open(
            ProbeLog::default(),
            EngineOptions {
                isolation: IsolationLevel::Serializable,
                serializable_validation: SerializableValidation::ReadSet,
                ..options()
            },
        )
        .unwrap();
        let mut seed = db.begin().unwrap();
        seed.ensure_link_created(1).unwrap();
        seed.commit().unwrap();
        let mut writer = db.begin().unwrap();
        let reader = db.begin().unwrap();
        if scan {
            assert_eq!(reader.links_count(), 1);
        } else {
            assert!(reader.get_link(1).is_some());
        }
        writer.set_name(1, Some("changed".into())).unwrap();
        let outcomes = db.commit_group(vec![writer, reader]).unwrap();
        assert_eq!(outcomes[0], GroupCommitOutcome::Committed(2));
        assert!(
            matches!(&outcomes[1],GroupCommitOutcome::Aborted(cause) if cause.contains(if scan {"serializable"} else {"read"}))
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn constraint_aborts_do_not_prevent_later_valid_revisions_or_burn_revisions() {
    for constraint in [
        Constraint::References,
        Constraint::UniqueNames,
        Constraint::UniquePairs,
    ] {
        let log = ProbeLog::default();
        let db = Engine::<u32, _>::open(
            log.clone(),
            EngineOptions {
                constraints: vec![constraint],
                ..options()
            },
        )
        .unwrap();
        let mut first = db.begin().unwrap();
        let id = first.create_link(0, 0).unwrap();
        first.set_name(id, Some("same".into())).unwrap();
        let mut bad = db.begin().unwrap();
        let b = bad
            .create_link(
                if constraint == Constraint::References {
                    999
                } else {
                    0
                },
                0,
            )
            .unwrap();
        bad.set_name(b, Some("same".into())).unwrap();
        let mut last = db.begin().unwrap();
        let c = last.create_link(id, 0).unwrap();
        last.set_name(c, Some("last".into())).unwrap();
        let result = db.commit_group(vec![first, bad, last]).unwrap();
        assert_eq!(result[0], GroupCommitOutcome::Committed(1));
        assert!(matches!(result[1], GroupCommitOutcome::Aborted(_)));
        assert_eq!(result[2], GroupCommitOutcome::Committed(2));
        assert!(!db.snapshot().unwrap().rows().contains_key(&b));
        let reopened = Engine::<u32, _>::open(log.clone(), options()).unwrap();
        assert_eq!(
            reopened.snapshot().unwrap().rows(),
            db.snapshot().unwrap().rows()
        );
        let mut next = reopened.begin().unwrap();
        assert_eq!(next.create_link(0, 0).unwrap(), 4);
        next.commit().unwrap();
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn admission_ownership_and_retention_survive_groups() {
    for concurrency in [
        ConcurrencyControl::Exclusive,
        ConcurrencyControl::Pessimistic,
    ] {
        let db = Engine::<u32, _>::open(
            ProbeLog::default(),
            EngineOptions {
                concurrency,
                history: HistoryPolicy::Last(1),
                ..options()
            },
        )
        .unwrap();
        let mut seed = db.begin().unwrap();
        seed.ensure_link_created(1).unwrap();
        seed.commit().unwrap();
        let pinned = db.snapshot().unwrap();
        let mut first = db.begin().unwrap();
        first.set_name(1, Some("first".into())).unwrap();
        let mut members = vec![first];
        if concurrency == ConcurrencyControl::Pessimistic {
            let mut second = db.begin().unwrap();
            assert!(second.set_name(1, Some("blocked".into())).is_err());
            second.create_link(1, 0).unwrap();
            members.push(second);
        }
        assert!(db
            .commit_group(members)
            .unwrap()
            .iter()
            .all(|r| matches!(r, GroupCommitOutcome::Committed(_))));
        assert!(db.snapshot_at(1).is_err());
        assert_eq!(pinned.rows().len(), 1);
        let mut following = db.begin().unwrap();
        following.set_name(1, Some("next".into())).unwrap();
        following.commit().unwrap();
    }
}

#[derive(Clone)]
struct FaultLog(Arc<Mutex<FaultState>>);
struct FaultState {
    records: Vec<String>,
    stable: Vec<String>,
    append: usize,
    phase: &'static str,
    index: usize,
    after: bool,
    write_through: bool,
}
impl TransitionLogStore for FaultLog {
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.0.lock().unwrap().records.clone())
    }
    fn append_log_entry(&mut self, record: &str) -> Result<(), LinkError> {
        let mut s = self.0.lock().unwrap();
        s.append += 1;
        let fail = s.phase == "append" && s.append == s.index;
        if fail && !s.after {
            return Err(LinkError::Transaction("before append".into()));
        }
        s.records.push(record.into());
        if s.write_through {
            s.stable = s.records.clone();
        }
        if fail {
            return Err(LinkError::Transaction("after append".into()));
        }
        Ok(())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        let mut s = self.0.lock().unwrap();
        if s.phase == "flush" && !s.after {
            return Err(LinkError::Transaction("before flush".into()));
        }
        s.stable = s.records.clone();
        if s.phase == "flush" {
            return Err(LinkError::Transaction("after flush".into()));
        }
        Ok(())
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn uncertain_io_preserves_known_aborts_and_reopens_the_independent_durable_prefix() {
    for phase in ["append", "flush"] {
        for index in 1..=2 {
            for after in [false, true] {
                for write_through in [false, true] {
                    if phase == "flush" && index != 1 {
                        continue;
                    }
                    let log = FaultLog(Arc::new(Mutex::new(FaultState {
                        records: vec![],
                        stable: vec![],
                        append: 0,
                        phase,
                        index,
                        after,
                        write_through,
                    })));
                    let db = Engine::<u32, _>::open(
                        log.clone(),
                        EngineOptions {
                            isolation: IsolationLevel::ReadCommitted,
                            constraints: vec![Constraint::References],
                            ..options()
                        },
                    )
                    .unwrap();
                    let observer = db.begin().unwrap();
                    let mut first = db.begin().unwrap();
                    let a = first.create_link(0, 0).unwrap();
                    first.set_name(a, Some("first".into())).unwrap();
                    let mut bad = db.begin().unwrap();
                    bad.create_link(999, 0).unwrap();
                    let mut last = db.begin().unwrap();
                    last.create_link(a, 0).unwrap();
                    let reader = db.begin().unwrap();
                    let outcomes = db.commit_group(vec![first, bad, last, reader]).unwrap();
                    assert!(matches!(outcomes[1], GroupCommitOutcome::Aborted(_)));
                    for i in [0, 2, 3] {
                        assert!(
                            matches!(&outcomes[i],GroupCommitOutcome::Indeterminate(message) if message.contains("reopen"))
                        );
                    }
                    assert_eq!(observer.links_count(), 0);
                    assert!(db.snapshot().is_err());
                    assert!(db.begin().is_err());
                    assert!(db.commit_group(vec![]).is_err());
                    drop(observer);
                    let stable = log.0.lock().unwrap().stable.clone();
                    let expected = if phase == "flush" && after {
                        2
                    } else if write_through {
                        if phase == "flush" {
                            2
                        } else {
                            index - 1 + usize::from(after)
                        }
                    } else {
                        0
                    };
                    assert_eq!(stable.len(), expected);
                    let reopened =
                        Engine::<u32, _>::open(MemoryLog { entries: stable }, options()).unwrap();
                    assert_eq!(reopened.snapshot().unwrap().revision, expected as u64);
                    assert_eq!(reopened.snapshot().unwrap().rows().len(), expected);
                    if expected > 0 {
                        assert_eq!(
                            reopened.snapshot().unwrap().get(a).unwrap().name.as_deref(),
                            Some("first")
                        );
                    }
                    let mut next = reopened.begin().unwrap();
                    next.create_link(0, 0).unwrap();
                    assert_eq!(next.commit().unwrap(), expected as u64 + 1);
                }
            }
        }
    }
}
