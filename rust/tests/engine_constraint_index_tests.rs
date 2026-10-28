//! Same logical constraints with an independently selected incremental cost.
use link_cli::engine::{
    ConcurrencyControl, Constraint, ConstraintValidation, Engine, EngineOptions,
    GroupCommitOutcome, HistoryPolicy, IsolationLevel, MemoryLog, RevisionEncoding,
    SerializableValidation, StateStructure,
};
use link_cli::LinksStorage;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn indexed_constraints_allow_atomic_swaps_and_preserve_rejected_candidates() {
    let db = Engine::<u32, _>::open(
        MemoryLog::default(),
        EngineOptions {
            constraint_validation: ConstraintValidation::Indexed,
            constraints: vec![
                Constraint::References,
                Constraint::UniquePairs,
                Constraint::UniqueNames,
            ],
            ..Default::default()
        },
    )
    .unwrap();
    let mut seed = db.begin().unwrap();
    for id in 1..=3 {
        seed.ensure_link_created(id).unwrap();
        seed.update_link(id, id, id).unwrap();
        seed.set_name(id, Some(format!("name {id}"))).unwrap();
    }
    seed.commit().unwrap();
    let mut swap = db.begin().unwrap();
    swap.update_link(1, 2, 2).unwrap();
    swap.set_name(1, Some("name 2".into())).unwrap();
    swap.update_link(2, 1, 1).unwrap();
    swap.set_name(2, Some("name 1".into())).unwrap();
    swap.commit().unwrap();
    let before = db.snapshot().unwrap();
    let mut bad = db.begin().unwrap();
    bad.delete_link(1).unwrap();
    assert!(bad.commit().unwrap_err().to_string().contains("reference"));
    assert_eq!(db.snapshot().unwrap().rows(), before.rows());
    let mut following = db.begin().unwrap();
    following.delete_link(1).unwrap();
    following.delete_link(2).unwrap();
    following.commit().unwrap();
    assert_eq!(db.snapshot().unwrap().links().len(), 1);
}

fn options(structure: StateStructure, encoding: RevisionEncoding, mask: u8) -> EngineOptions {
    EngineOptions {
        state_structure: structure,
        revision_encoding: encoding,
        constraint_validation: ConstraintValidation::Indexed,
        constraints: [
            Constraint::References,
            Constraint::UniquePairs,
            Constraint::UniqueNames,
        ]
        .into_iter()
        .enumerate()
        .filter_map(|(bit, c)| (mask & (1 << bit) != 0).then_some(c))
        .collect(),
        history: HistoryPolicy::All,
        isolation: IsolationLevel::Snapshot,
        ..Default::default()
    }
}
fn seed(db: &Engine<u32, MemoryLog>) {
    let mut seed = db.begin().unwrap();
    for (id, name) in [(1, "a"), (2, "雪\0"), (3, "")] {
        seed.ensure_link_created(id).unwrap();
        seed.update_link(id, id, id).unwrap();
        seed.set_name(id, Some(name.into())).unwrap();
    }
    seed.commit().unwrap();
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_independent_oracle_checks_every_constraint_subset_and_single_row_edit() {
    // Three rows; 6,400 finite candidates, including missing references, zero,
    // self/cyclic references, duplicate pairs and null/empty/Unicode names.
    let initial = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    seed(&initial);
    let original = initial.with_log(|log| Ok(log.clone())).unwrap();
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        for encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
            for mask in 0..8 {
                for id in 1..=2 {
                    for source in 0..=4 {
                        for target in 0..=4 {
                            for name in [None, Some("a"), Some("雪\0"), Some("")] {
                                let mut rows = [
                                    (1, 1, 1, Some("a")),
                                    (2, 2, 2, Some("雪\0")),
                                    (3, 3, 3, Some("")),
                                ];
                                rows[(id - 1) as usize] = (id, source, target, name);
                                let references = rows.iter().all(|(_, s, t, _)| {
                                    [*s, *t]
                                        .into_iter()
                                        .all(|r| r == 0 || rows.iter().any(|row| row.0 == r))
                                });
                                let pairs = rows
                                    .iter()
                                    .map(|(_, s, t, _)| (*s, *t))
                                    .collect::<std::collections::BTreeSet<_>>()
                                    .len()
                                    == rows.len();
                                let names: Vec<_> = rows.iter().filter_map(|row| row.3).collect();
                                let names = names
                                    .iter()
                                    .collect::<std::collections::BTreeSet<_>>()
                                    .len()
                                    == names.len();
                                let expected = (mask & 1 == 0 || references)
                                    && (mask & 2 == 0 || pairs)
                                    && (mask & 4 == 0 || names);
                                let db = Engine::<u32, _>::open(
                                    original.clone(),
                                    options(structure, encoding, mask),
                                )
                                .unwrap();
                                let mut tx = db.begin().unwrap();
                                tx.update_link(id, source, target).unwrap();
                                tx.set_name(id, name.map(str::to_owned)).unwrap();
                                assert_eq!(tx.commit().is_ok(),expected,"{structure:?}/{encoding:?}/{mask} {id} {source} {target} {name:?}");
                                if !expected {
                                    assert_eq!(db.snapshot().unwrap().revision, 1);
                                    assert_eq!(
                                        db.with_log(|log| Ok(log.entries.len())).unwrap(),
                                        1
                                    );
                                    let mut following = db.begin().unwrap();
                                    following.set_name(3, Some("following".into())).unwrap();
                                    following.commit().unwrap();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn indexes_count_both_reference_positions_and_only_reject_the_final_candidate() {
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        for encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
            let db = Engine::<u32, _>::open(MemoryLog::default(), options(structure, encoding, 1))
                .unwrap();
            seed(&db);
            let mut refs = db.begin().unwrap();
            refs.update_link(2, 1, 1).unwrap();
            refs.update_link(3, 1, 0).unwrap();
            refs.commit().unwrap();
            let mut one = db.begin().unwrap();
            one.update_link(2, 2, 0).unwrap();
            one.commit().unwrap();
            let mut bad = db.begin().unwrap();
            bad.delete_link(1).unwrap();
            assert!(bad.commit().is_err());
            let mut remove = db.begin().unwrap();
            remove.delete_link(1).unwrap();
            remove.delete_link(3).unwrap();
            remove.commit().unwrap();
            let mut transient = db.begin().unwrap();
            let id = transient.create_link(99, 99).unwrap();
            transient.delete_link(id).unwrap();
            transient.commit().unwrap();
            let log = db.with_log(|log| Ok(log.clone())).unwrap();
            let reopened = Engine::<u32, _>::open(log, options(structure, encoding, 1)).unwrap();
            let mut bad = reopened.begin().unwrap();
            bad.update_link(2, 1, 0).unwrap();
            assert!(bad.commit().is_err());
            let mut good = reopened.begin().unwrap();
            good.ensure_link_created(1).unwrap();
            good.update_link(1, 2, 0).unwrap();
            good.update_link(2, 1, 0).unwrap();
            good.commit().unwrap();
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn groups_branches_recovery_and_retained_snapshots_keep_their_own_index_roots() {
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        for encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
            let opts = options(structure, encoding, 7);
            let db = Engine::<u32, _>::open(MemoryLog::default(), opts.clone()).unwrap();
            seed(&db);
            let pinned = db.snapshot().unwrap();
            let mut a = db.begin().unwrap();
            let mut b = db.begin().unwrap();
            a.set_name(1, Some("group".into())).unwrap();
            b.set_name(2, Some("group".into())).unwrap();
            let outcomes = db.commit_group(vec![a, b]).unwrap();
            assert_eq!(outcomes[0], GroupCommitOutcome::Committed(2));
            assert!(matches!(outcomes[1], GroupCommitOutcome::Aborted(_)));
            assert_eq!(pinned.get(1).unwrap().name.as_deref(), Some("a"));
            for validation in [ConstraintValidation::Scan, ConstraintValidation::Indexed] {
                let mut child_options = opts.clone();
                child_options.constraint_validation = validation;
                child_options.constraints.clear();
                let mut branch = db.fork(MemoryLog::default(), child_options).unwrap();
                let mut edit = branch.engine().begin().unwrap();
                edit.set_name(2, Some("group".into())).unwrap();
                edit.commit().unwrap();
                assert!(branch.merge().is_err());
                assert_eq!(
                    db.snapshot().unwrap().revision,
                    2 + u64::from(validation == ConstraintValidation::Indexed)
                );
                let mut fix = branch.engine().begin().unwrap();
                fix.set_name(2, Some(format!("child {validation:?}")))
                    .unwrap();
                fix.commit().unwrap();
                branch.merge().unwrap();
                branch.merge().unwrap();
            }
            let log = db.with_log(|log| Ok(log.clone())).unwrap();
            for validation in [ConstraintValidation::Scan, ConstraintValidation::Indexed] {
                let mut reopen = opts.clone();
                reopen.constraint_validation = validation;
                let reopened = Engine::<u32, _>::open(log.clone(), reopen).unwrap();
                let mut bad = reopened.begin().unwrap();
                bad.set_name(3, Some("group".into())).unwrap();
                assert!(bad.commit().is_err());
                let mut transfer = reopened.begin().unwrap();
                transfer.set_name(1, None).unwrap();
                transfer.set_name(3, Some("group".into())).unwrap();
                transfer.commit().unwrap();
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn recovery_rejects_invalid_historical_constraints_even_if_later_records_fix_them() {
    let db = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    seed(&db);
    let mut bad = db.begin().unwrap();
    bad.set_name(2, Some("a".into())).unwrap();
    bad.commit().unwrap();
    let mut fix = db.begin().unwrap();
    fix.set_name(2, Some("fixed".into())).unwrap();
    fix.commit().unwrap();
    let log = db.with_log(|log| Ok(log.clone())).unwrap();
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        assert!(Engine::<u32, _>::open(
            log.clone(),
            options(structure, RevisionEncoding::Changes, 4)
        )
        .is_err());
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn indexed_constraints_preserve_independent_admission_and_read_validation() {
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        for encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
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
                        let db = Engine::<u32, _>::open(
                            MemoryLog::default(),
                            EngineOptions {
                                concurrency,
                                isolation,
                                serializable_validation: validation,
                                ..options(structure, encoding, 7)
                            },
                        )
                        .unwrap();
                        seed(&db);
                        let mut reader = db.begin().unwrap();
                        assert!(reader.get_link(1).is_some());
                        if concurrency == ConcurrencyControl::Exclusive {
                            assert!(db.begin().is_err());
                        } else {
                            let mut writer = db.begin().unwrap();
                            writer.set_name(1, Some("concurrent".into())).unwrap();
                            writer.commit().unwrap();
                        }
                        reader.set_name(2, Some("reader".into())).unwrap();
                        assert_eq!(
                            reader.commit().is_ok(),
                            concurrency == ConcurrencyControl::Exclusive
                                || isolation != IsolationLevel::Serializable
                        );
                        let mut bad = db.begin().unwrap();
                        bad.set_name(3, db.snapshot().unwrap().get(1).unwrap().name.clone())
                            .unwrap();
                        assert!(bad.commit().is_err());
                        let mut following = db.begin().unwrap();
                        following.set_name(3, Some("following".into())).unwrap();
                        following.commit().unwrap();
                    }
                }
            }
        }
    }
}
