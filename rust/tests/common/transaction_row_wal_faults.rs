//! Child module of transaction_row_wal_tests; shared native/actual-WASM fixtures.
use super::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn selected_images_complete_unobserved_effects_for_history_and_committed_redo() {
    for explicit in [false, true] {
        let (tx, control) = open(Image::default());
        let mut tx = tx
            .with_row_undo(link_cli::transactions::RowUndoPolicy::WriteAheadImage)
            .unwrap();
        let (first, _) = seed(&mut tx);
        let before = capture(&mut tx);
        let saved_before = control.borrow().image.clone();
        let start = tx.journal().len();
        if explicit {
            tx.begin_transaction().unwrap();
        }
        control.borrow_mut().unobserved = true;
        tx.update(first, 21, 22).unwrap();
        tx.commit().unwrap();
        let after = capture(&mut tx);
        assert_ne!(after, before);
        let records = tx.journal()[start..].to_vec();
        for record in records.iter().rev() {
            tx.revert_record(record).unwrap();
        }
        assert_eq!(capture(&mut tx), before, "complete history undo/{explicit}");
        for record in &records {
            tx.apply_record(record).unwrap();
        }
        assert_eq!(capture(&mut tx), after, "complete history redo/{explicit}");
        let mut redo = control.borrow().image.clone();
        redo.rows = saved_before.rows;
        redo.names = saved_before.names;
        let start = records.first().unwrap().sequence;
        redo.journal.retain(|entry| {
            !entry
                .strip_prefix("__transactions:applied:")
                .is_some_and(|value| value.parse::<i64>().unwrap() >= start)
        });
        for _ in 0..2 {
            let (mut reopened, state) = open(redo);
            assert_eq!(capture(&mut reopened), after, "committed redo/{explicit}");
            redo = state.borrow().image.clone();
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn preparation_row_and_commit_failures_recover_independent_images_and_cleanup() {
    for (label, point, skip, attempted_commit) in [
        ("prepare append before", "append-before", 0, false),
        ("prepare append after", "append-after", 0, false),
        ("prepare flush before", "log-before", 0, false),
        ("prepare flush after", "log-after", 0, false),
        ("row before", "row-before", 0, false),
        ("row after", "row-after", 0, false),
        ("row append before", "append-before", 1, false),
        ("row append after", "append-after", 1, false),
        ("commit append before", "append-before", 2, true),
        ("commit append after", "append-after", 2, true),
        ("commit flush before", "log-before", 1, true),
        ("commit flush after", "log-after", 1, true),
        ("checkpoint before", "data-before", 0, true),
        ("checkpoint after", "data-after", 0, true),
        ("checkpoint log before", "log-before", 2, true),
        ("checkpoint log after", "log-after", 2, true),
    ] {
        for explicit in [false, true] {
            let (tx, control) = open(Image::default());
            let mut tx = tx
                .with_row_undo(link_cli::transactions::RowUndoPolicy::WriteAheadImage)
                .unwrap();
            let (first, _) = seed(&mut tx);
            let before = capture(&mut tx);
            let old_commits = control
                .borrow()
                .image
                .journal
                .iter()
                .filter(|entry| entry.starts_with("__transactions:commit:"))
                .cloned()
                .collect::<Vec<_>>();
            if explicit {
                tx.begin_transaction().unwrap();
            }
            {
                let mut state = control.borrow_mut();
                state.point = Some(point);
                state.skip = skip;
            }
            assert!(
                tx.update(first, 21, 22).and_then(|_| tx.commit()).is_err(),
                "{label}/{explicit}"
            );
            assert!(control.borrow().point.is_none(), "fault must fire {label}");
            let mut image = control.borrow().failed.clone().expect("fault image");
            let committed = image.journal.iter().any(|entry| {
                entry.starts_with("__transactions:commit:") && !old_commits.contains(entry)
            });
            for reopen in 0..2 {
                let (mut reopened, state) = open(image);
                assert_eq!(
                    reopened.inner().get_link(first),
                    Some(Link::new(
                        first,
                        if committed { 21 } else { 11 },
                        if committed { 22 } else { 12 }
                    )),
                    "{label}/{explicit}/{reopen}"
                );
                assert_eq!(
                    reopened
                        .inner_mut()
                        .capture_names()
                        .unwrap()
                        .unwrap()
                        .get(&first)
                        .map(String::as_str),
                    Some("original 雪")
                );
                let id = reopened.create(31, 32).unwrap();
                reopened
                    .set_name(id, &format!("following {reopen}"))
                    .unwrap();
                image = state.borrow().image.clone();
            }
            if explicit || attempted_commit {
                assert!(tx.create(41, 42).is_err(), "owner lost {label}");
                assert!(tx.set_name(first, "blocked").is_err());
                assert!(tx.begin_transaction().is_err());
            }
            tx.rollback().unwrap();
            tx.rollback().unwrap();
            if attempted_commit {
                assert_eq!(tx.inner().get_link(first), Some(Link::new(first, 21, 22)));
            } else {
                assert_eq!(capture(&mut tx), before, "live undo {label}/{explicit}");
            }
            tx.create(41, 42).unwrap();
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn failed_image_undo_retains_owner_until_complete_restoration_and_flush() {
    for point in [
        "row-before",
        "row-after",
        "name-before",
        "name-after",
        "data-before",
        "data-after",
        "append-before",
        "append-after",
        "log-before",
        "log-after",
    ] {
        let (tx, control) = open(Image::default());
        let mut tx = tx
            .with_row_undo(link_cli::transactions::RowUndoPolicy::WriteAheadImage)
            .unwrap();
        let (first, _) = seed(&mut tx);
        let before = capture(&mut tx);
        tx.begin_transaction().unwrap();
        tx.update(first, 21, 22).unwrap();
        tx.set_name(first, "changed").unwrap();
        for retry in 0..2 {
            control.borrow_mut().point = Some(point);
            assert!(tx.rollback().is_err(), "{point}/{retry}");
            assert!(control.borrow().point.is_none());
            assert!(tx.create(41, 42).is_err());
            assert!(tx.commit().is_err());
            assert!(tx.begin_transaction().is_err());
            let image = control.borrow().failed.clone().unwrap();
            let (mut reopened, _) = open(image);
            assert_eq!(
                capture(&mut reopened),
                before,
                "recover undo {point}/{retry}"
            );
        }
        tx.rollback().unwrap();
        tx.rollback().unwrap();
        assert_eq!(capture(&mut tx), before);
        tx.create(41, 42).unwrap();
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unreported_effects_from_name_writes_also_have_complete_history() {
    for (explicit, same_name) in [(false, false), (true, false), (false, true), (true, true)] {
        let (tx, control) = open(Image::default());
        let mut tx = tx
            .with_row_undo(link_cli::transactions::RowUndoPolicy::WriteAheadImage)
            .unwrap();
        let (first, _) = seed(&mut tx);
        let before = capture(&mut tx);
        let start = tx.journal().len();
        if explicit {
            tx.begin_transaction().unwrap();
        }
        control.borrow_mut().hidden_on_name = true;
        tx.set_name(first, if same_name { "original 雪" } else { "changed" })
            .unwrap();
        tx.commit().unwrap();
        let after = capture(&mut tx);
        let records = tx.journal()[start..].to_vec();
        for record in records.iter().rev() {
            tx.revert_record(record).unwrap();
        }
        assert_eq!(capture(&mut tx), before);
        for record in &records {
            tx.apply_record(record).unwrap();
        }
        assert_eq!(capture(&mut tx), after);
        let (mut reopened, _) = open(control.borrow().image.clone());
        assert_eq!(capture(&mut reopened), after);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn failed_result_capture_and_correction_append_retain_complete_image_undo() {
    for point in ["capture", "append-before", "append-after"] {
        for explicit in [false, true] {
            let (tx, control) = open(Image::default());
            let mut tx = tx
                .with_row_undo(link_cli::transactions::RowUndoPolicy::WriteAheadImage)
                .unwrap();
            let (first, _) = seed(&mut tx);
            let before = capture(&mut tx);
            if explicit {
                tx.begin_transaction().unwrap();
            }
            {
                let mut state = control.borrow_mut();
                state.unobserved = true;
                if point == "capture" {
                    state.fail_after_unobserved = true;
                } else {
                    state.point = Some(point);
                    state.skip = 2;
                }
            }
            assert!(tx.update(first, 21, 22).is_err(), "{point}/{explicit}");
            let mut image = control.borrow().failed.clone().expect("fault must fire");
            for _ in 0..2 {
                let (mut restored, state) = open(image);
                assert_eq!(capture(&mut restored), before);
                image = state.borrow().image.clone();
            }
            if explicit {
                assert!(tx.commit().is_err());
                assert!(tx.set_name(first, "blocked").is_err());
            }
            tx.rollback().unwrap();
            tx.rollback().unwrap();
            assert_eq!(capture(&mut tx), before);
            tx.create(31, 32).unwrap();
        }
    }
}
