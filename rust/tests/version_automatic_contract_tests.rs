//! Ordinary automatic ownership and read-only shortcuts remain portable.
use anyhow::Result;
use link_cli::{
    CommitMode, LinkError, LogRetentionPolicy, TransactionsDecorator, TransitionLogStore,
    VersionControlDecorator,
};
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;

fn open() -> Result<(
    test_store::NamedFiles,
    test_store::NamedFiles,
    test_store::NamedFiles,
    VersionControlDecorator,
)> {
    let (data_files, data) = test_store::named()?;
    let (log_files, log) = test_store::named()?;
    let (branch_files, branches) = test_store::named()?;
    let tx = TransactionsDecorator::new(
        data,
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )?;
    Ok((
        data_files,
        log_files,
        branch_files,
        VersionControlDecorator::new(tx, branches, false)?,
    ))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn noops_do_not_create_an_owner_or_a_decision() -> Result<()> {
    let (_data, _log, _branches, mut vc) = open()?;
    let id = vc.create_and_update(11, 12)?;
    let sequence = vc.current_sequence();
    let entries = vc.transactions_mut().log_store_mut().read_log_entries()?;
    assert_eq!(vc.try_ensure_created(0)?, 0);
    assert_eq!(vc.try_ensure_created(id)?, id);
    assert_eq!(vc.get_or_create(11, 12)?, id);
    assert_eq!(
        vc.transactions_mut().log_store_mut().read_log_entries()?,
        entries
    );
    assert_eq!(vc.current_sequence(), sequence);
    vc.branch("following", None)?;
    vc.begin_transaction()?;
    vc.rollback()?;
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn automatic_precommit_errors_release_admission_without_changing_history() -> Result<()> {
    let (_data, _log, _branches, mut vc) = open()?;
    let id = vc.create_and_update(11, 12)?;
    let original = vc.current_sequence();
    for delete in [false, true] {
        let error = if delete {
            vc.delete(100)
        } else {
            vc.update(100, 21, 22)
        }
        .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<LinkError>(),
            Some(LinkError::StorageError(message)) if message == "Link not found: 100"
        ));
        assert_eq!(vc.current_sequence(), original);
        assert_eq!(vc.get(id).unwrap().source, 11);
        vc.begin_transaction()?;
        vc.rollback()?;
    }
    vc.branch("following", None)?;
    vc.update(id, 31, 32)?;
    let following = vc.current_sequence();
    vc.checkout(original)?;
    assert_eq!(vc.get(id).unwrap().source, 11);
    vc.checkout(following)?;
    assert_eq!(vc.get(id).unwrap().source, 31);
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn explicit_precommit_errors_keep_the_callers_undo_state() -> Result<()> {
    let (_data, _log, _branches, mut vc) = open()?;
    let id = vc.create_and_update(11, 12)?;
    let original = vc.current_sequence();
    vc.begin_transaction()?;
    vc.update(id, 21, 22)?;
    assert!(vc.update(100, 31, 32).is_err());
    assert_eq!(vc.get(id).unwrap().source, 21);
    assert_eq!(vc.current_sequence(), original);
    assert!(vc.branch("blocked", None).is_err());
    vc.rollback()?;
    assert_eq!(vc.get(id).unwrap().source, 11);
    assert_eq!(vc.current_sequence(), original);
    vc.update(id, 31, 32)?;
    assert_eq!(
        vc.current_sequence(),
        vc.transactions().last_logged_sequence()
    );
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn automatic_entry_points_publish_and_release_their_owner() -> Result<()> {
    let (_data, _log, _branches, mut vc) = open()?;
    let id = vc.create(11, 12)?;
    assert_eq!(
        vc.current_sequence(),
        vc.transactions().last_logged_sequence()
    );
    vc.branch("created", None)?;
    let composite = vc.create_and_update(21, 22)?;
    vc.update_observed(composite, 31, 32, &mut |_, _| {})?;
    vc.branch("updated", None)?;
    assert_eq!(
        vc.current_sequence(),
        vc.transactions().last_logged_sequence()
    );
    vc.delete_observed(composite, &mut |_, _| {})?;
    vc.try_ensure_created(20)?;
    let following = vc.get_or_create(41, 42)?;
    assert_eq!(
        vc.current_sequence(),
        vc.transactions().last_logged_sequence()
    );
    vc.branch("following", None)?;
    vc.checkout(0)?;
    assert!(vc.all().is_empty());
    vc.switch_branch("following")?;
    assert!(vc.exists(id));
    assert!(vc.exists(20));
    assert!(vc.exists(following));
    assert!(!vc.exists(composite));
    Ok(())
}
