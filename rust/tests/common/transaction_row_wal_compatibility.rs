//! Finite replay adapters may acknowledge an effect without performing it.
use super::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unnamed_images_restore_rows_without_replacing_external_names() {
    let (mut seeded, state) = open(Image::default());
    seed(&mut seeded);
    let names = seeded.inner_mut().capture_names().unwrap().unwrap();
    let mut image = state.borrow().image.clone();
    image.rows = vec![Link::new(1, 99, 99), Link::new(8, 1, 1)];
    image.journal = vec![
        "__transactions:transition:v3|undo|00000000000000000000000000000001|0|1,11,12|-".into(),
    ];
    for reopen in 0..2 {
        let (mut restored, saved) = open(image);
        assert_eq!(restored.inner().all_links(), vec![Link::new(1, 11, 12)]);
        assert_eq!(
            restored.inner_mut().capture_names().unwrap().unwrap(),
            names
        );
        image = saved.borrow().image.clone();
        if reopen == 1 {
            restored.create_and_update(41, 42).unwrap();
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn incomplete_row_or_name_restoration_keeps_cleanup_ownership() {
    for names in [false, true] {
        let (tx, state) = open(Image::default());
        let mut tx = tx
            .with_row_undo(link_cli::RowUndoPolicy::WriteAheadImage)
            .unwrap();
        let (first, _) = seed(&mut tx);
        let before = capture(&mut tx);
        tx.begin_transaction().unwrap();
        tx.update(first, 21, 22).unwrap();
        tx.set_name(first, "changed").unwrap();
        if names {
            state.borrow_mut().ignore_name_restore = Some(first);
        } else {
            state.borrow_mut().ignore_row_restore = Some(first);
        }
        for retry in 0..2 {
            let error = tx.rollback().unwrap_err();
            assert!(
                error.to_string().contains("complete undo image"),
                "{names}/{retry}: {error}"
            );
            assert!(state.borrow().ignored_calls > 0);
            assert!(tx.create(41, 42).is_err());
            assert!(tx.update(first, 41, 42).is_err());
            assert!(tx.set_name(first, "blocked").is_err());
            assert!(tx.begin_transaction().is_err());
            assert!(tx.commit().is_err());
            assert!(tx.recover().is_err());
            // Persist the partial logical replay independently of the wrapper.
            tx.inner_mut().flush().unwrap();
            let mut image = state.borrow().image.clone();
            for _ in 0..2 {
                let (mut reopened, saved) = open(image);
                assert_eq!(capture(&mut reopened), before, "{names}/{retry}");
                image = saved.borrow().image.clone();
            }
        }
        state.borrow_mut().ignore_row_restore = None;
        state.borrow_mut().ignore_name_restore = None;
        tx.rollback().unwrap();
        tx.rollback().unwrap();
        assert_eq!(capture(&mut tx), before);
        tx.create_and_update(41, 42).unwrap();
        tx.set_name(first, "following").unwrap();
        let after = capture(&mut tx);
        let (mut reopened, _) = open(state.borrow().image.clone());
        assert_eq!(capture(&mut reopened), after);
    }
}
