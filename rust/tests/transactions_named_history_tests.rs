//! Names must participate in the legacy owner's ordinary undo and history.
use anyhow::Result;
use link_cli::{
    CommitMode, LogRetentionPolicy, NamedTypeLinks, TransactionsDecorator, VersionControlDecorator,
};
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;

fn make_tx() -> (TransactionsDecorator, Vec<test_store::NamedFiles>) {
    let (data_owner, data) = test_store::named().unwrap();
    let (log_owner, log) = test_store::named().unwrap();
    (
        TransactionsDecorator::new(
            data,
            log,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap(),
        vec![data_owner, log_owner],
    )
}

fn undo_named_write(kind: &str) -> Result<()> {
    let (mut tx, _owners) = make_tx();
    let first = tx.create(11, 12)?;
    let second = tx.create(first, 22)?;
    NamedTypeLinks::set_name(&mut tx, first, "first 雪|\0\n")?;
    NamedTypeLinks::set_name(&mut tx, second, "second")?;
    tx.begin_transaction()?;
    match kind {
        "rename" => {
            NamedTypeLinks::set_name(&mut tx, first, "changed")?;
        }
        "remove" => NamedTypeLinks::remove_name(&mut tx, first)?,
        "move" => {
            NamedTypeLinks::set_name(&mut tx, second, "first 雪|\0\n")?;
        }
        "delete" => {
            tx.delete(second)?;
        }
        "cascade" => {
            tx.delete(first)?;
        }
        _ => unreachable!(),
    }
    tx.rollback()?;
    assert!(tx.exists(first));
    assert!(tx.exists(second));
    assert_eq!(
        NamedTypeLinks::get_name(&mut tx, first)?.as_deref(),
        Some("first 雪|\0\n")
    );
    assert_eq!(
        NamedTypeLinks::get_name(&mut tx, second)?.as_deref(),
        Some("second")
    );
    assert_eq!(
        NamedTypeLinks::get_by_name(&mut tx, "first 雪|\0\n")?,
        Some(first)
    );
    assert_eq!(
        NamedTypeLinks::get_by_name(&mut tx, "second")?,
        Some(second)
    );
    Ok(())
}

macro_rules! undo_case {
    ($name:ident, $kind:literal) => {
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        fn $name() -> Result<()> {
            undo_named_write($kind)
        }
    };
}
undo_case!(rollback_restores_renamed_link, "rename");
undo_case!(rollback_restores_removed_name, "remove");
undo_case!(rollback_restores_both_owners_after_name_move, "move");
undo_case!(rollback_restores_deleted_links_name, "delete");
undo_case!(rollback_restores_cascaded_links_names, "cascade");

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn name_only_versions_advance_and_replay() -> Result<()> {
    let (tx, _owners) = make_tx();
    let (_branch_owner, branches) = test_store::named()?;
    let mut vc = VersionControlDecorator::new(tx, branches, false)?;
    let id = vc.create(11, 12)?;
    NamedTypeLinks::set_name(&mut vc, id, "original")?;
    let before = vc.current_sequence();
    NamedTypeLinks::set_name(&mut vc, id, "changed 雪")?;
    let after = vc.current_sequence();
    assert!(
        after > before,
        "a name-only write must advance version history"
    );
    vc.checkout(before)?;
    assert_eq!(
        NamedTypeLinks::get_name(&mut vc, id)?.as_deref(),
        Some("original")
    );
    vc.checkout(after)?;
    assert_eq!(
        NamedTypeLinks::get_name(&mut vc, id)?.as_deref(),
        Some("changed 雪")
    );
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn rollback_of_allocation_preserves_preexisting_external_name() -> Result<()> {
    let (mut tx, _owners) = make_tx();
    NamedTypeLinks::set_name(&mut tx, 77, "external 雪")?;
    assert!(!tx.exists(77));
    tx.begin_transaction()?;
    tx.ensure_created(77)?;
    assert!(tx.exists(77));
    tx.rollback()?;
    assert!(!tx.exists(77));
    assert_eq!(
        NamedTypeLinks::get_name(&mut tx, 77)?.as_deref(),
        Some("external 雪")
    );
    assert_eq!(
        NamedTypeLinks::get_by_name(&mut tx, "external 雪")?,
        Some(77)
    );
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn empty_and_zero_address_names_keep_distinct_presence_and_owners() -> Result<()> {
    let (mut tx, _owners) = make_tx();
    NamedTypeLinks::set_name(&mut tx, 0, "")?;
    NamedTypeLinks::set_name(&mut tx, 77, "Type")?;
    tx.begin_transaction()?;
    NamedTypeLinks::set_name(&mut tx, 77, "")?;
    assert_eq!(NamedTypeLinks::get_name(&mut tx, 0)?, None);
    tx.rollback()?;
    assert_eq!(NamedTypeLinks::get_name(&mut tx, 0)?.as_deref(), Some(""));
    assert_eq!(NamedTypeLinks::get_by_name(&mut tx, "")?, Some(0));
    assert_eq!(
        NamedTypeLinks::get_name(&mut tx, 77)?.as_deref(),
        Some("Type")
    );
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn selected_undo_image_keeps_versions_and_idle_policy_selection() -> Result<()> {
    use link_cli::transactions::RowUndoPolicy;
    let (mut tx, _owners) = make_tx();
    assert_eq!(tx.row_undo_policy(), RowUndoPolicy::Observed);
    tx.set_row_undo_policy(RowUndoPolicy::WriteAheadImage)?;
    tx.begin_transaction()?;
    assert!(tx.set_row_undo_policy(RowUndoPolicy::Observed).is_err());
    assert_eq!(tx.row_undo_policy(), RowUndoPolicy::WriteAheadImage);
    tx.rollback()?;
    let (_branch_owner, branches) = test_store::named()?;
    let mut vc = VersionControlDecorator::new(tx, branches, false)?;
    let id = vc.create_and_update(11, 12)?;
    vc.set_name(id, "original")?;
    let before = vc.current_sequence();
    let records = vc.transactions().journal().len();
    // Selected preparation must not turn a genuine no-op into a version.
    vc.set_name(id, "original")?;
    vc.remove_name(77)?;
    assert_eq!(vc.current_sequence(), before);
    assert_eq!(vc.transactions().journal().len(), records);
    vc.begin_transaction()?;
    vc.set_name(id, "changed")?;
    vc.update(id, 21, 22)?;
    vc.ensure_created(5);
    vc.set_name(5, "allocated")?;
    vc.commit()?;
    let after = vc.current_sequence();
    assert_eq!(after, vc.transactions().last_logged_sequence());
    for _ in 0..2 {
        vc.checkout(before)?;
        assert_eq!(vc.get_name(id)?.as_deref(), Some("original"));
        assert_eq!(vc.get(id).unwrap().source, 11);
        assert!(!vc.exists(5));
        vc.checkout(after)?;
        assert_eq!(vc.get_name(id)?.as_deref(), Some("changed"));
        assert_eq!(vc.get_name(5)?.as_deref(), Some("allocated"));
        assert_eq!(vc.get(id).unwrap().source, 21);
    }
    Ok(())
}
