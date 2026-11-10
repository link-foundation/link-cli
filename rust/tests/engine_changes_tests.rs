//! Finite semantic/cost controls for independently selected logical redo.
use link_cli::engine::{
    ConcurrencyControl, Constraint, Engine, EngineOptions, GroupCommitOutcome, HistoryPolicy,
    IsolationLevel, MemoryLog, RevisionEncoding, SerializableValidation, StateStructure,
};
use link_cli::{LinksStorage, TransitionLogStore};

fn options(state_structure: StateStructure, revision_encoding: RevisionEncoding) -> EngineOptions {
    EngineOptions {
        state_structure,
        revision_encoding,
        history: HistoryPolicy::All,
        ..Default::default()
    }
}
fn encodings() -> [RevisionEncoding; 2] {
    [RevisionEncoding::Snapshot, RevisionEncoding::Changes]
}
fn structures() -> [StateStructure; 3] {
    [
        StateStructure::Flat,
        StateStructure::Shared,
        StateStructure::SortedRuns,
    ]
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn selected_encoding_serializes_only_changed_rows_without_changing_noop_or_allocation_semantics() {
    for structure in structures() {
        for encoding in encodings() {
            let opts = options(structure, encoding);
            let db = Engine::<u64, _>::open(MemoryLog::default(), opts.clone()).unwrap();
            let mut seed = db.begin().unwrap();
            for id in 1..=128 {
                seed.ensure_link_created(id).unwrap();
                seed.set_name(id, Some(format!("unchanged {id} {}", "雪".repeat(16))))
                    .unwrap();
            }
            seed.commit().unwrap();
            let original = db.snapshot().unwrap();
            for step in 1..=16 {
                let mut tx = db.begin().unwrap();
                tx.update_link(1, step, 0).unwrap();
                tx.commit().unwrap();
            }
            let log = db.with_log(|log| Ok(log.clone())).unwrap();
            assert_eq!(log.entries.len(), 17);
            let mut encoded_rows = 0;
            for record in &log.entries[1..] {
                let fields: Vec<_> = record.split('|').collect();
                let (tag, index, count) = if encoding == RevisionEncoding::Changes {
                    ("engine2", 4, 1)
                } else {
                    ("engine1", 3, 128)
                };
                assert_eq!(fields[0], tag);
                assert_eq!(fields[index].split(';').count(), count);
                encoded_rows += count;
            }
            assert_eq!(
                encoded_rows,
                if encoding == RevisionEncoding::Changes {
                    16
                } else {
                    2048
                }
            );
            assert_eq!(original.get(1).unwrap().link.source, 0);
            for id in 2..=128 {
                assert_eq!(original.get(id), db.snapshot().unwrap().get(id));
            }
            // Equal resulting rows still represent an accepted write revision, but
            // canonical changes omit redundant puts and absent tombstones.
            let mut equal = db.begin().unwrap();
            equal.update_link(1, 16, 0).unwrap();
            assert_eq!(equal.commit().unwrap(), 18);
            let mut transient = db.begin().unwrap();
            assert_eq!(transient.create_link(0, 0).unwrap(), 129);
            transient.delete_link(129).unwrap();
            assert_eq!(transient.commit().unwrap(), 19);
            let log = db.with_log(|log| Ok(log.clone())).unwrap();
            if encoding == RevisionEncoding::Changes {
                assert_eq!(log.entries[17].split('|').nth(4), Some(""));
                assert_eq!(log.entries[18].split('|').nth(4), Some(""));
            }
            for reopen_encoding in encodings() {
                for reopen_structure in structures() {
                    let reopened = Engine::<u64, _>::open(
                        log.clone(),
                        options(reopen_structure, reopen_encoding),
                    )
                    .unwrap();
                    assert_eq!(
                        reopened.snapshot().unwrap().rows(),
                        db.snapshot().unwrap().rows()
                    );
                    let mut following = reopened.begin().unwrap();
                    assert_eq!(following.create_link(0, 0).unwrap(), 130);
                    following.commit().unwrap();
                }
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn groups_and_evicted_merge_bases_replay_both_encodings_and_mixed_state_choices() {
    for structure in structures() {
        for branch_structure in structures() {
            for encoding in encodings() {
                for branch_encoding in encodings() {
                    let mut opts = options(structure, encoding);
                    opts.isolation = IsolationLevel::Snapshot;
                    let db = Engine::<u64, _>::open(MemoryLog::default(), opts.clone()).unwrap();
                    let mut seed = db.begin().unwrap();
                    seed.ensure_link_created(1).unwrap();
                    seed.ensure_link_created(2).unwrap();
                    seed.commit().unwrap();
                    let pinned = db.snapshot().unwrap();
                    let mut a = db.begin().unwrap();
                    let mut b = db.begin().unwrap();
                    a.set_name(1, Some("one".into())).unwrap();
                    b.set_name(2, Some("two".into())).unwrap();
                    assert_eq!(
                        db.commit_group(vec![a, b]).unwrap(),
                        vec![
                            GroupCommitOutcome::Committed(2),
                            GroupCommitOutcome::Committed(3)
                        ]
                    );
                    let mut child_options = options(branch_structure, branch_encoding);
                    child_options.history = HistoryPolicy::None;
                    let child = db
                        .fork(MemoryLog::default(), child_options.clone())
                        .unwrap();
                    let base = child.merge_base_record();
                    assert!(base.starts_with("engine1|"));
                    let mut remote = child.engine().begin().unwrap();
                    remote.delete_link(2).unwrap();
                    remote.ensure_link_created(u64::MAX).unwrap();
                    remote.set_name(u64::MAX, Some("".into())).unwrap();
                    remote.commit().unwrap();
                    drop(remote);
                    assert!(child.engine().snapshot_at(1).is_err());
                    let journal = child.into_engine().into_log().ok().unwrap();
                    if branch_encoding == RevisionEncoding::Changes {
                        assert!(db
                            .resume_branch(
                                journal.clone(),
                                child_options.clone(),
                                &journal.entries[0]
                            )
                            .is_err());
                    }
                    let mut resumed = db.resume_branch(journal, child_options, &base).unwrap();
                    assert_eq!(resumed.merge().unwrap(), 4);
                    assert_eq!(resumed.merge().unwrap(), 4);
                    assert_eq!(pinned.links().len(), 2);
                    assert!(pinned.get(1).unwrap().name.is_none());
                    let log = db.with_log(|log| Ok(log.clone())).unwrap();
                    let reopened = Engine::<u64, _>::open(log, opts).unwrap();
                    assert!(reopened.snapshot().unwrap().get(2).is_none());
                    assert_eq!(
                        reopened
                            .snapshot()
                            .unwrap()
                            .get(u64::MAX)
                            .unwrap()
                            .name
                            .as_deref(),
                        Some("")
                    );
                    let mut next = reopened.begin().unwrap();
                    assert!(next.create_link(0, 0).is_err());
                    next.set_name(1, Some("following".into())).unwrap();
                    next.commit().unwrap();
                }
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn changed_encoding_preserves_admission_visibility_validation_constraints_and_history() {
    for structure in structures() {
        for concurrency in [
            ConcurrencyControl::Optimistic,
            ConcurrencyControl::Exclusive,
            ConcurrencyControl::Pessimistic,
        ] {
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
                    let opts = EngineOptions {
                        concurrency,
                        isolation,
                        serializable_validation: validation,
                        ..options(structure, RevisionEncoding::Changes)
                    };
                    let db = Engine::<u32, _>::open(MemoryLog::default(), opts).unwrap();
                    let mut seed = db.begin().unwrap();
                    seed.ensure_link_created(1).unwrap();
                    seed.ensure_link_created(2).unwrap();
                    seed.commit().unwrap();
                    let mut reader = db.begin().unwrap();
                    assert_eq!(reader.all_links().len(), 2);
                    if concurrency == ConcurrencyControl::Exclusive {
                        assert!(db.begin().is_err());
                        reader.rollback();
                        continue;
                    }
                    let mut writer = db.begin().unwrap();
                    writer.set_name(1, Some("changed".into())).unwrap();
                    writer.commit().unwrap();
                    assert_eq!(
                        reader.get_by_name("changed"),
                        (isolation == IsolationLevel::ReadCommitted).then_some(1)
                    );
                    reader.set_name(2, Some("reader".into())).unwrap();
                    assert_eq!(
                        reader.commit().is_err(),
                        isolation == IsolationLevel::Serializable
                    );
                }
            }
        }
        for constraint in [
            Constraint::References,
            Constraint::UniquePairs,
            Constraint::UniqueNames,
        ] {
            let opts = EngineOptions {
                constraints: vec![constraint],
                ..options(structure, RevisionEncoding::Changes)
            };
            let db = Engine::<u32, _>::open(MemoryLog::default(), opts).unwrap();
            let mut bad = db.begin().unwrap();
            bad.create_link(0, 0).unwrap();
            bad.create_link(0, 0).unwrap();
            match constraint {
                Constraint::References => {
                    bad.update_link(1, 99, 0).unwrap();
                }
                Constraint::UniqueNames => {
                    bad.set_name(1, Some("x".into())).unwrap();
                    bad.set_name(2, Some("x".into())).unwrap();
                }
                Constraint::UniquePairs => {}
            }
            assert!(bad.commit().is_err());
            assert!(db
                .with_log(|log| log.read_log_entries())
                .unwrap()
                .is_empty());
            let mut good = db.begin().unwrap();
            good.create_link(0, 0).unwrap();
            good.commit().unwrap();
        }
        for history in [
            HistoryPolicy::None,
            HistoryPolicy::Last(1),
            HistoryPolicy::All,
        ] {
            let opts = EngineOptions {
                history,
                ..options(structure, RevisionEncoding::Changes)
            };
            let db = Engine::<u32, _>::open(MemoryLog::default(), opts.clone()).unwrap();
            let mut seed = db.begin().unwrap();
            seed.create_link(0, 0).unwrap();
            seed.commit().unwrap();
            let pinned = db.snapshot().unwrap();
            let mut edit = db.begin().unwrap();
            edit.set_name(1, Some("new".into())).unwrap();
            edit.commit().unwrap();
            assert!(pinned.get(1).unwrap().name.is_none());
            assert_eq!(db.snapshot_at(1).is_ok(), history == HistoryPolicy::All);
            let log = db.with_log(|log| Ok(log.clone())).unwrap();
            let reopened = Engine::<u32, _>::open(log, opts).unwrap();
            assert_eq!(
                reopened.snapshot().unwrap().get(1).unwrap().name.as_deref(),
                Some("new")
            );
        }
    }
}
