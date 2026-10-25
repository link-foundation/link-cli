//! Recovery ordering uses independently authored mandatory images and row WAL.
use super::*;
const PREFIX: &str = "__transactions:transition:";
fn image(owner: u128, base: i64, source: u32, target: u32, names: &str) -> String {
    format!("{PREFIX}v3|undo|{owner:032x}|{base}|1,{source},{target}|{names}")
}
fn row(owner: u128, sequence: i64, before: (u32, u32), after: (u32, u32)) -> String {
    format!(
        "{PREFIX}v1|{owner:032x}|{sequence}|0|1|1,{},{}|1,{},{}",
        before.0, before.1, after.0, after.1
    )
}
fn decision(owner: u128) -> String {
    format!("__transactions:commit:{owner:032x}")
}
fn verify(entries: Vec<String>, expected: Link) {
    let (mut tx, state) = open(Image {
        rows: vec![Link::new(1, 99, 99), Link::new(8, 1, 1)],
        names: Vec::new(),
        journal: entries,
    });
    assert_eq!(tx.inner().all_links(), vec![expected]);
    tx.set_name(77, "following").unwrap();
    let saved = state.borrow().image.clone();
    let (mut reopened, _) = open(saved);
    assert_eq!(reopened.inner().all_links(), vec![expected]);
    assert_eq!(
        reopened
            .inner_mut()
            .capture_names()
            .unwrap()
            .unwrap()
            .get(&77)
            .map(String::as_str),
        Some("following")
    );
    reopened.update(1, 41, 42).unwrap();
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn restoring_image_redoes_later_commits_even_when_marked_applied() {
    verify(
        vec![
            image(1, 0, 11, 12, ""),
            row(2, 1, (11, 12), (31, 32)),
            decision(2),
            "__transactions:applied:1".into(),
        ],
        Link::new(1, 31, 32),
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn image_restore_preserves_older_checkpoint_gap_and_mixed_pending_undo_order() {
    verify(
        vec![
            row(7, 1, (11, 12), (21, 22)),
            decision(7),
            image(1, 1, 21, 22, ""),
            row(9, 2, (21, 22), (99, 99)),
            row(3, 3, (21, 22), (31, 32)),
            decision(3),
            "__transactions:applied:3".into(),
            image(4, 3, 31, 32, ""),
        ],
        Link::new(1, 31, 32),
    );
    verify(
        vec![row(9, 1, (11, 12), (21, 22)), image(1, 1, 21, 22, "")],
        Link::new(1, 11, 12),
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn identical_image_retries_collapse_independently_of_physical_position() {
    let first = image(1, 0, 11, 12, "");
    verify(
        vec![
            first.clone(),
            row(2, 1, (11, 12), (31, 32)),
            decision(2),
            first,
        ],
        Link::new(1, 31, 32),
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn conflicting_or_late_owner_images_fail_before_rows_or_indexes_change() {
    for entries in [
        vec![image(1, 0, 11, 12, ""), image(1, 0, 21, 22, "")],
        vec![row(1, 1, (11, 12), (21, 22)), image(1, 1, 11, 12, "")],
        vec![image(1, 0, 11, 12, ""), image(2, 0, 21, 22, "")],
    ] {
        let (mut tx, _) = open(Image::default());
        seed(&mut tx);
        let before = capture(&mut tx);
        let history = tx.journal();
        tx.log_store_mut().entries = entries;
        for _ in 0..2 {
            assert!(tx.recover().is_err());
            assert_eq!(capture(&mut tx), before);
            assert_eq!(tx.journal(), history);
            assert!(tx.create(1, 2).is_err());
        }
    }
}
