use link_cli::engine::{
    Engine, EngineOptions, HistoryPolicy, MemoryLog, RevisionEncoding, StateStructure,
};
use link_cli::{LinkError, LinksStorage};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_checkpoint_vectors_replay_or_reject() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/engine-checkpoint-vectors.json"
    ))
    .unwrap();
    for case in vectors["valid"].as_array().unwrap() {
        let entries: Vec<String> = serde_json::from_value(case.clone()).unwrap();
        let engine = Engine::<u32, _>::open(
            MemoryLog {
                entries: entries.clone(),
            },
            EngineOptions::default(),
        )
        .unwrap();
        if entries.len() == 1 {
            assert_eq!(engine.export_checkpoint().unwrap(), entries);
        }
    }
    for case in vectors["invalidU32"].as_array().unwrap() {
        let entries = serde_json::from_value(case.clone()).unwrap();
        assert!(
            Engine::<u32, _>::open(MemoryLog { entries }, EngineOptions::default()).is_err(),
            "accepted {case}"
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checkpoints_preserve_selected_history_names_holes_and_following_changes() {
    for structure in [
        StateStructure::Flat,
        StateStructure::Shared,
        StateStructure::SortedRuns,
    ] {
        for encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
            for history in [
                HistoryPolicy::None,
                HistoryPolicy::Last(2),
                HistoryPolicy::All,
            ] {
                let options = EngineOptions {
                    state_structure: structure,
                    revision_encoding: encoding,
                    history,
                    ..Default::default()
                };
                let engine = Engine::<u32, _>::open(MemoryLog::default(), options.clone()).unwrap();
                for revision in 1..=6 {
                    let mut tx = engine.begin().unwrap();
                    if revision == 1 {
                        tx.create_link(0, 0).unwrap();
                    }
                    if revision == 3 {
                        tx.ensure_link_created(7).unwrap();
                        tx.delete_link(7).unwrap();
                    }
                    tx.set_name(1, Some(format!("雪 {revision}"))).unwrap();
                    tx.commit().unwrap();
                }
                let original = engine.with_log(|log| Ok(log.entries.clone())).unwrap();
                let exported = engine.export_checkpoint().unwrap();
                assert!(exported[0].starts_with("engine3|"));
                assert_eq!(
                    exported.len(),
                    match history {
                        HistoryPolicy::None => 1,
                        HistoryPolicy::Last(_) => 2,
                        HistoryPolicy::All => 7,
                    }
                );
                assert_eq!(
                    engine.with_log(|log| Ok(log.entries.clone())).unwrap(),
                    original
                );
                let copy =
                    Engine::<u32, _>::open(MemoryLog { entries: exported }, options).unwrap();
                for revision in 0..=6 {
                    match (engine.snapshot_at(revision), copy.snapshot_at(revision)) {
                        (Ok(a), Ok(b)) => assert_eq!(a.rows(), b.rows()),
                        (Err(_), Err(_)) => {}
                        _ => panic!("history changed at {revision}"),
                    }
                }
                let mut tx = copy.begin().unwrap();
                assert_eq!(tx.create_link(1, 0).unwrap(), 8);
                tx.commit().unwrap();
                let journal = copy.with_log(|log| Ok(log.entries.clone())).unwrap();
                let reopened = Engine::<u32, _>::open(
                    MemoryLog { entries: journal },
                    EngineOptions::default(),
                )
                .unwrap();
                assert_eq!(reopened.snapshot().unwrap().revision, 7);
                assert_eq!(
                    reopened.snapshot().unwrap().get(1).unwrap().name.as_deref(),
                    Some("雪 6")
                );
                assert!(reopened.snapshot().unwrap().get(8).is_some());
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn export_excludes_pending_reservations_and_rejects_uncertain_owner() {
    let engine = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut pending = engine.begin().unwrap();
    pending.ensure_link_created(99).unwrap();
    let copy = Engine::<u32, _>::open(
        MemoryLog {
            entries: engine.export_checkpoint().unwrap(),
        },
        EngineOptions::default(),
    )
    .unwrap();
    assert_eq!(copy.begin().unwrap().create_link(0, 0).unwrap(), 1);
    pending.rollback();
    let mut write = engine.begin().unwrap();
    assert_eq!(write.create_link(0, 0).unwrap(), 100);
    write.commit().unwrap();
    let mut pending = engine.begin().unwrap();
    pending.ensure_link_created(199).unwrap();
    let copy = Engine::<u32, _>::open(
        MemoryLog {
            entries: engine.export_checkpoint().unwrap(),
        },
        EngineOptions::default(),
    )
    .unwrap();
    assert_eq!(copy.begin().unwrap().create_link(0, 0).unwrap(), 101);
    assert_eq!(engine.snapshot().unwrap().revision, 1);
    pending.rollback();
    assert!(engine
        .with_log::<()>(|_| Err(LinkError::Transaction("unknown".into())))
        .is_err());
    assert!(engine.export_checkpoint().is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checkpoint_replay_enforces_each_selected_constraint() {
    use link_cli::engine::{Constraint, ConstraintValidation};
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/engine-checkpoint-vectors.json"
    ))
    .unwrap();
    for structure in [
        StateStructure::Flat,
        StateStructure::Shared,
        StateStructure::SortedRuns,
    ] {
        for validation in [ConstraintValidation::Scan, ConstraintValidation::Indexed] {
            for (key, constraint) in [
                ("references", Constraint::References),
                ("uniquePairs", Constraint::UniquePairs),
                ("uniqueNames", Constraint::UniqueNames),
            ] {
                let entries: Vec<String> =
                    serde_json::from_value(vectors["constraintViolations"][key].clone()).unwrap();
                assert!(Engine::<u32, _>::open(
                    MemoryLog {
                        entries: entries.clone()
                    },
                    EngineOptions::default()
                )
                .is_ok());
                let options = EngineOptions {
                    state_structure: structure,
                    constraint_validation: validation,
                    constraints: vec![constraint],
                    ..Default::default()
                };
                assert!(Engine::<u32, _>::open(MemoryLog { entries }, options).is_err());
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checkpointed_branches_resume_only_with_retained_exact_merge_bases() {
    for history in [HistoryPolicy::All, HistoryPolicy::Last(2)] {
        let parent =
            Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
        let mut seed = parent.begin().unwrap();
        seed.create_link(0, 0).unwrap();
        seed.commit().unwrap();
        let options = EngineOptions {
            history,
            ..Default::default()
        };
        let branch = parent.fork(MemoryLog::default(), options.clone()).unwrap();
        let base = branch.merge_base_record();
        let mut edit = branch.engine().begin().unwrap();
        edit.ensure_link_created(7).unwrap();
        edit.delete_link(7).unwrap();
        edit.set_name(1, Some("checkpointed branch 雪".into()))
            .unwrap();
        edit.commit().unwrap();
        drop(edit);
        let records = branch.engine().export_checkpoint().unwrap();
        // Dropping the base's retained revision must not invent membership.
        let pruned = Engine::<u32, _>::open(
            MemoryLog {
                entries: records.clone(),
            },
            EngineOptions::default(),
        )
        .unwrap();
        assert!(parent
            .resume_branch(
                MemoryLog {
                    entries: pruned.export_checkpoint().unwrap()
                },
                options.clone(),
                &base
            )
            .is_err());
        let mut resumed = parent
            .resume_branch(MemoryLog { entries: records }, options, &base)
            .unwrap();
        assert_eq!(resumed.merge().unwrap(), 2);
        assert_eq!(
            parent.snapshot().unwrap().get(1).unwrap().name.as_deref(),
            Some("checkpointed branch 雪")
        );
        let mut following = parent.begin().unwrap();
        assert_eq!(following.create_link(0, 0).unwrap(), 8);
        following.commit().unwrap();
    }
}
