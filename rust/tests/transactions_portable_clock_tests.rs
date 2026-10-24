//! The same in-memory legacy transaction API must run on native and JS/WASM.
use anyhow::Result;
use link_cli::{
    CommitMode, LinkStorage, LogRetentionPolicy, NamedTypesDecorator, TransactionsDecorator,
    VersionControlDecorator,
};

fn named() -> NamedTypesDecorator {
    NamedTypesDecorator::from_link_storages(
        LinkStorage::in_memory(false),
        LinkStorage::in_memory(false),
    )
}

fn transactions() -> Result<TransactionsDecorator> {
    Ok(TransactionsDecorator::new(
        named(),
        named(),
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )?)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn in_memory_transactions_have_real_timestamps_and_unique_ids() -> Result<()> {
    let mut tx = transactions()?;
    let first = tx.begin_transaction()?;
    assert!(
        first.started_ms > 1_000_000_000,
        "timestamp is Unix milliseconds"
    );
    let created = tx.create_and_update(11, 12)?;
    tx.commit()?;
    let log = tx.log();
    assert_eq!(log.len(), 2);
    assert!(log
        .iter()
        .all(|transition| transition.transaction_id == first.id
            && transition.timestamp_ms > 1_000_000_000));
    let second = tx.begin_transaction()?;
    assert_ne!(first.id, second.id);
    tx.update(created, 21, 22)?;
    tx.rollback()?;
    assert_eq!(tx.get(created).unwrap().source, 11);
    tx.create_and_update(31, 32)?;
    tx.save()?;
    assert_eq!(tx.all().len(), 2);
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn in_memory_legacy_version_transactions_keep_branch_and_tag_history() -> Result<()> {
    let mut vc = VersionControlDecorator::new(transactions()?, named(), false)?;
    let baseline = vc.create_and_update(11, 12)?;
    let base = vc.current_sequence();
    vc.branch("topic:parent=😀", None)?;
    vc.switch_branch("topic:parent=😀")?;
    vc.begin_transaction()?;
    let created = vc.create_and_update(21, 22)?;
    vc.commit()?;
    let head = vc.current_sequence();
    assert!(head > base);
    vc.tag("release=😀", None)?;
    assert_eq!(vc.try_get_tag("release=😀"), Some(head));
    vc.recover()?;
    assert_eq!(vc.current_branch(), "topic:parent=😀");
    assert_eq!(vc.try_get_tag("release=😀"), Some(head));
    vc.checkout(base)?;
    assert!(vc.exists(baseline));
    assert!(!vc.exists(created));
    vc.checkout(head)?;
    assert!(vc.exists(created));
    vc.switch_branch("main")?;
    assert_eq!(vc.current_sequence(), base);
    assert!(!vc.exists(created));
    vc.switch_branch("topic:parent=😀")?;
    assert!(vc.exists(created));
    vc.save()?;
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn transaction_recovery_preserves_an_open_owner() -> Result<()> {
    let mut tx = transactions()?;
    tx.begin_transaction()?;
    let id = tx.create_and_update(11, 12)?;
    assert!(
        tx.recover().is_err(),
        "recovery cannot undo an owned transaction"
    );
    tx.commit()?;
    tx.recover()?;
    assert_eq!(tx.get(id).unwrap().source, 11);
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn version_recovery_preserves_an_open_owner() -> Result<()> {
    let mut vc = VersionControlDecorator::new(transactions()?, named(), false)?;
    vc.begin_transaction()?;
    let id = vc.create_and_update(11, 12)?;
    assert!(
        vc.recover().is_err(),
        "recovery cannot replace owned version metadata"
    );
    vc.commit()?;
    let head = vc.current_sequence();
    vc.recover()?;
    assert_eq!(vc.current_sequence(), head);
    assert_eq!(vc.get(id).unwrap().source, 11);
    Ok(())
}
