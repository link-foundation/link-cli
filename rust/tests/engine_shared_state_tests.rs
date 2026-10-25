//! Finite independent controls for selected immutable state sharing.
use link_cli::engine::{
    ConcurrencyControl, Constraint, Engine, EngineOptions, GroupCommitOutcome, HistoryPolicy,
    IsolationLevel, MemoryLog, SerializableValidation, StateStructure,
};
use link_cli::{GenericLink, LinkError, LinksStorage, TransitionLogStore};
use std::collections::BTreeMap;
use std::{cell::RefCell, rc::Rc};

fn options(state_structure: StateStructure) -> EngineOptions {
    EngineOptions {
        state_structure,
        history: HistoryPolicy::All,
        ..Default::default()
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn selected_structure_preserves_unchanged_row_identity_and_old_snapshots() {
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        let db = Engine::<u64, _>::open(MemoryLog::default(), options(structure)).unwrap();
        let mut seed = db.begin().unwrap();
        for id in 1..=256 {
            seed.ensure_link_created(id).unwrap();
            seed.set_name(id, Some(format!("row {id} 雪"))).unwrap();
        }
        seed.commit().unwrap();
        let first = db.snapshot().unwrap();
        let mut previous = first.clone();
        for value in 1..=16 {
            let mut tx = db.begin().unwrap();
            tx.update_link(1, value, 0).unwrap();
            tx.commit().unwrap();
            let current = db.snapshot().unwrap();
            for id in 2..=256 {
                assert_eq!(current.get(id), previous.get(id));
                assert_eq!(
                    std::ptr::eq(current.get(id).unwrap(), previous.get(id).unwrap()),
                    structure == StateStructure::Shared,
                    "{structure:?}: unchanged row {id} at revision {value}"
                );
            }
            assert_eq!(first.get(1).unwrap().link.source, 0);
            previous = current;
        }
        assert_eq!(db.snapshot_at(1).unwrap().get(1), first.get(1));
        assert_eq!(
            first.rows().len(),
            256,
            "legacy materialized map remains available"
        );
        assert_eq!(first.rows().get(&1).unwrap().link.source, 0);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn both_structures_match_a_finite_independent_row_and_name_oracle() {
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        let db = Engine::<u64, _>::open(MemoryLog::default(), options(structure)).unwrap();
        let mut expected = BTreeMap::new();
        let mut saved = Vec::new();
        for step in 0..96u64 {
            let id = 1 + (step * 7 % 13);
            let mut tx = db.begin().unwrap();
            let mut candidate = expected.clone();
            match step % 4 {
                0 | 1 => {
                    tx.ensure_link_created(id).unwrap();
                    tx.update_link(id, step, id).unwrap();
                    let name = (step % 3 != 0).then(|| format!("{step};|雪\0"));
                    tx.set_name(id, name.clone()).unwrap();
                    candidate.insert(id, (step, id, name));
                }
                2 => {
                    if candidate.remove(&id).is_some() {
                        tx.delete_link(id).unwrap();
                    }
                }
                _ => assert_eq!(tx.get_link(id).is_some(), expected.contains_key(&id)),
            }
            if step % 7 == 0 {
                tx.rollback();
            } else {
                tx.commit().unwrap();
                expected = candidate;
            }
            let actual = db.snapshot().unwrap();
            let values: BTreeMap<_, _> = actual
                .iter()
                .map(|(id, row)| (*id, (row.link.source, row.link.target, row.name.clone())))
                .collect();
            assert_eq!(values, expected, "{structure:?}, step {step}");
            saved.push((actual, expected.clone()));
        }
        for (snapshot, expected) in saved {
            assert_eq!(snapshot.links().len(), expected.len());
            for (id, (source, target, name)) in expected {
                let row = snapshot.get(id).unwrap();
                assert_eq!(row.link, GenericLink::new(id, source, target));
                assert_eq!(row.name, name);
            }
        }
        let records = db.with_log(|log| Ok(log.clone())).unwrap();
        for reopen_structure in [StateStructure::Flat, StateStructure::Shared] {
            let reopened =
                Engine::<u64, _>::open(records.clone(), options(reopen_structure)).unwrap();
            assert_eq!(
                reopened.snapshot().unwrap().rows(),
                db.snapshot().unwrap().rows()
            );
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn structure_choice_is_independent_of_admission_isolation_and_read_validation() {
    for structure in [StateStructure::Flat, StateStructure::Shared] {
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
                    let db = Engine::<u64, _>::open(
                        MemoryLog::default(),
                        EngineOptions {
                            concurrency,
                            isolation,
                            serializable_validation: validation,
                            ..options(structure)
                        },
                    )
                    .unwrap();
                    let mut tx = db.begin().unwrap();
                    tx.ensure_link_created(1).unwrap();
                    tx.ensure_link_created(2).unwrap();
                    tx.commit().unwrap();
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
                    // Closed-session reads keep the established base-snapshot behavior.
                    assert_eq!(
                        reader.get_name(1),
                        if isolation != IsolationLevel::Serializable {
                            Some("changed".into())
                        } else {
                            None
                        }
                    );
                }
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn groups_branches_and_reopening_preserve_mixed_structure_choices() {
    for parent_structure in [StateStructure::Flat, StateStructure::Shared] {
        for branch_structure in [StateStructure::Flat, StateStructure::Shared] {
            let config = EngineOptions {
                isolation: IsolationLevel::Snapshot,
                ..options(parent_structure)
            };
            let db = Engine::<u64, _>::open(MemoryLog::default(), config.clone()).unwrap();
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
            assert!(pinned.get(1).unwrap().name.is_none());
            let branch = db
                .fork(MemoryLog::default(), options(branch_structure))
                .unwrap();
            let base = branch.merge_base_record();
            let mut remote = branch.engine().begin().unwrap();
            remote.delete_link(2).unwrap();
            remote.ensure_link_created(u64::MAX).unwrap();
            remote.commit().unwrap();
            drop(remote);
            let branch_log = branch.into_engine().into_log().ok().unwrap();
            let mut branch = db
                .resume_branch(branch_log, options(branch_structure), &base)
                .unwrap();
            assert_eq!(branch.merge().unwrap(), 4);
            assert_eq!(branch.merge().unwrap(), 4);
            let current = db.snapshot().unwrap();
            assert!(current.get(2).is_none());
            assert!(current.get(u64::MAX).is_some());
            let mut discarded = db.begin().unwrap();
            discarded.set_name(1, Some("discarded".into())).unwrap();
            discarded.rollback();
            let log = db.with_log(|log| Ok(log.clone())).unwrap();
            let reopened = Engine::<u64, _>::open(log, config).unwrap();
            assert_eq!(reopened.snapshot().unwrap().rows(), current.rows());
            assert_eq!(reopened.snapshot_at(1).unwrap().links().len(), 2);
            let mut following = reopened.begin().unwrap();
            following.set_name(1, Some("following".into())).unwrap();
            following.commit().unwrap();
            assert_eq!(current.get(1).unwrap().name.as_deref(), Some("one"));
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn selected_constraints_fail_without_changing_prior_snapshots_or_history() {
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        for constraint in [
            Constraint::References,
            Constraint::UniquePairs,
            Constraint::UniqueNames,
        ] {
            let db = Engine::<u64, _>::open(
                MemoryLog::default(),
                EngineOptions {
                    constraints: vec![constraint],
                    ..options(structure)
                },
            )
            .unwrap();
            let before = db.snapshot().unwrap();
            let mut tx = db.begin().unwrap();
            let a = tx.create_link(0, 0).unwrap();
            let b = tx.create_link(0, 0).unwrap();
            match constraint {
                Constraint::References => {
                    tx.update_link(a, 99, 0).unwrap();
                }
                Constraint::UniqueNames => {
                    tx.set_name(a, Some("duplicate".into())).unwrap();
                    tx.set_name(b, Some("duplicate".into())).unwrap();
                }
                Constraint::UniquePairs => {}
            }
            assert!(tx.commit().is_err());
            assert!(before.links().is_empty());
            assert_eq!(db.snapshot().unwrap().revision, 0);
            assert!(db.with_log(|log| Ok(log.entries.is_empty())).unwrap());
            let mut following = db.begin().unwrap();
            let c = following.create_link(0, 0).unwrap();
            following.set_name(c, Some("valid".into())).unwrap();
            following.commit().unwrap();
            assert_eq!(db.snapshot().unwrap().links().len(), 1);
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn existing_portable_records_reopen_under_both_structures_without_wire_changes() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/engine-revision-vectors.json"
    ))
    .unwrap();
    let records: Vec<String> = vectors["valid"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record.as_str().unwrap().into())
        .collect();
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        let db = Engine::<u32, _>::open(
            MemoryLog {
                entries: records.clone(),
            },
            options(structure),
        )
        .unwrap();
        assert_eq!(
            db.snapshot_at(1).unwrap().get(1).unwrap().name.as_deref(),
            Some("雪;|😀")
        );
        assert!(db.snapshot().unwrap().get(u32::MAX).is_some());
        assert_eq!(db.with_log(|log| Ok(log.entries.clone())).unwrap(), records);
    }
}

#[derive(Default)]
struct Faults {
    log: MemoryLog,
    append: bool,
    flush: bool,
}
struct Failing(Rc<RefCell<Faults>>);
impl TransitionLogStore for Failing {
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.0.borrow().log.entries.clone())
    }
    fn append_log_entry(&mut self, record: &str) -> Result<(), LinkError> {
        let mut state = self.0.borrow_mut();
        if state.append {
            return Err(LinkError::StorageError("append".into()));
        }
        state.log.entries.push(record.into());
        Ok(())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        if self.0.borrow().flush {
            Err(LinkError::StorageError("flush".into()))
        } else {
            Ok(())
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn uncertain_commits_preserve_pinned_state_and_reopen_the_actual_outcome() {
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        for flush in [false, true] {
            let fault = Rc::new(RefCell::new(Faults::default()));
            let db = Engine::<u64, _>::open(Failing(fault.clone()), options(structure)).unwrap();
            let mut seed = db.begin().unwrap();
            seed.create_link(0, 0).unwrap();
            seed.commit().unwrap();
            let pinned = db.snapshot().unwrap();
            fault.borrow_mut().append = !flush;
            fault.borrow_mut().flush = flush;
            let mut tx = db.begin().unwrap();
            tx.set_name(1, Some("uncertain".into())).unwrap();
            assert!(tx.commit().is_err());
            assert!(pinned.get(1).unwrap().name.is_none());
            assert!(db.snapshot().is_err());
            let reopened =
                Engine::<u64, _>::open(fault.borrow().log.clone(), options(structure)).unwrap();
            assert_eq!(
                reopened.snapshot().unwrap().get(1).unwrap().name.as_deref(),
                flush.then_some("uncertain")
            );
            let mut following = reopened.begin().unwrap();
            following.set_name(1, Some("following".into())).unwrap();
            following.commit().unwrap();
            assert!(pinned.get(1).unwrap().name.is_none());
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn retained_and_unretained_shared_snapshots_survive_noop_and_rollback_paths() {
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        for history in [
            HistoryPolicy::None,
            HistoryPolicy::Last(1),
            HistoryPolicy::All,
        ] {
            let db = Engine::<u64, _>::open(
                MemoryLog::default(),
                EngineOptions {
                    history,
                    ..options(structure)
                },
            )
            .unwrap();
            let mut seed = db.begin().unwrap();
            seed.create_link(0, 0).unwrap();
            seed.commit().unwrap();
            let pinned = db.snapshot().unwrap();
            let mut writer = db.begin().unwrap();
            writer.set_name(1, Some("changed".into())).unwrap();
            writer.commit().unwrap();
            assert!(pinned.get(1).unwrap().name.is_none());
            assert_eq!(db.snapshot_at(1).is_ok(), history == HistoryPolicy::All);
            let mut reader = db.begin().unwrap();
            assert_eq!(reader.commit().unwrap(), 2);
            assert_eq!(db.snapshot().unwrap().revision, 2);
            let mut discarded = db.begin().unwrap();
            discarded.delete_link(1).unwrap();
            discarded.rollback();
            assert_eq!(
                db.snapshot().unwrap().get(1).unwrap().name.as_deref(),
                Some("changed")
            );
            assert!(db.begin().unwrap().ensure_link_created(0).is_err());
        }
    }
}
