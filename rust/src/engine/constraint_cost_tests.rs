//! Retained immutable boundaries and selected costs without a public test API.
use super::*;
use crate::LinksStorage;

#[test]
fn only_selected_constraints_allocate_indexes_and_unchanged_keys_reuse_roots() {
    for validation in [ConstraintValidation::Scan, ConstraintValidation::Indexed] {
        for mask in 0..8 {
            let db = Engine::<u32, _>::open(
                MemoryLog::default(),
                EngineOptions {
                    constraint_validation: validation,
                    constraints: [
                        Constraint::References,
                        Constraint::UniquePairs,
                        Constraint::UniqueNames,
                    ]
                    .into_iter()
                    .enumerate()
                    .filter_map(|(bit, c)| (mask & (1 << bit) != 0).then_some(c))
                    .collect(),
                    state_structure: StateStructure::Shared,
                    revision_encoding: RevisionEncoding::Changes,
                    ..Default::default()
                },
            )
            .unwrap();
            let mut tx = db.begin().unwrap();
            for id in 1..=512 {
                tx.ensure_link_created(id).unwrap();
                tx.update_link(id, id, id).unwrap();
                tx.set_name(id, Some(format!("name {id}"))).unwrap();
            }
            tx.commit().unwrap();
            let old = db.state.lock().unwrap().constraint_index.clone();
            if validation == ConstraintValidation::Scan || mask == 0 {
                assert!(old.is_none());
                continue;
            }
            let old = old.unwrap();
            assert_eq!(old.references.is_some(), mask & 1 != 0);
            assert_eq!(old.pairs.is_some(), mask & 2 != 0);
            assert_eq!(old.names.is_some(), mask & 4 != 0);
            let mut edit = db.begin().unwrap();
            edit.set_name(1, Some("changed".into())).unwrap();
            edit.commit().unwrap();
            let state = db.state.lock().unwrap();
            let next = state.constraint_index.as_ref().unwrap();
            if let Some(pairs) = &old.pairs {
                assert!(pairs.ptr_eq(next.pairs.as_ref().unwrap()));
            }
            if let Some(refs) = &old.references {
                assert!(refs.ptr_eq(next.references.as_ref().unwrap()));
                assert_eq!(refs.get(&1), Some(&2));
            }
            if let Some(names) = &old.names {
                assert!(!names.ptr_eq(next.names.as_ref().unwrap()));
                assert_eq!(names.get(&"name 1".to_owned()), Some(&1));
                assert!(names.get(&"changed".to_owned()).is_none());
            }
        }
    }
}
