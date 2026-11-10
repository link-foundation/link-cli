//! An aborted record must never become an initial reachable version.
use anyhow::Result;
use link_cli::{
    CommitMode, LogRetentionPolicy, NamedTypes, TransactionsDecorator, VersionControlDecorator,
};
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;

#[cfg(not(target_arch = "wasm32"))]
fn named(files: &test_store::NamedFiles) -> Result<link_cli::NamedTypesDecorator> {
    link_cli::NamedTypesDecorator::with_names_database_path(files.0.path(), files.1.path(), false)
}
fn transactions(
    data: link_cli::NamedTypesDecorator,
    log: link_cli::NamedTypesDecorator,
) -> Result<TransactionsDecorator> {
    Ok(TransactionsDecorator::new(
        data,
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )?)
}
fn verify(history: &str, pending: bool) -> Result<()> {
    let (_data_files, data) = test_store::named()?;
    let (_log_files, log) = test_store::named()?;
    let (_branch_files, branches) = test_store::named()?;
    let mut tx = transactions(data, log)?;
    let mut id = 0;
    let mut expected_head = 0;
    if history != "all" {
        id = tx.create_and_update(11, 12)?;
        expected_head = tx.last_logged_sequence();
    }
    let transaction = tx.begin_transaction()?;
    if history == "all" {
        id = tx.create_and_update(21, 22)?;
    } else {
        tx.update(id, 21, 22)?;
    }
    let aborted: Vec<_> = tx
        .log()
        .iter()
        .filter(|change| change.transaction_id == transaction.id)
        .map(|change| change.sequence)
        .collect();
    assert!(!aborted.is_empty());
    if !pending {
        tx.rollback()?;
    }
    tx.save()?;
    let (data, log) = tx.into_inner();
    // Constructor recovery must undo a persisted pending transaction first.
    let mut tx = transactions(data, log)?;
    if history == "middle" {
        tx.update(id, 31, 32)?;
        expected_head = tx.last_logged_sequence();
    }
    let mut vc = VersionControlDecorator::new(tx, branches, false)?;
    for reopen in 0..2 {
        assert_eq!(vc.current_sequence(), expected_head);
        assert_eq!(vc.list_branches()[0].head, expected_head);
        for sequence in &aborted {
            assert!(
                vc.checkout(*sequence).is_err(),
                "aborted sequence {sequence} must not be reachable"
            );
            assert!(vc.branch("aborted", Some(*sequence)).is_err());
        }
        assert_eq!(vc.all().len(), usize::from(history != "all"));
        vc.checkout(0)?;
        assert!(vc.all().is_empty());
        vc.checkout(expected_head)?;
        assert_eq!(vc.all().len(), usize::from(history != "all"));
        if history != "all" {
            let row = vc.get(id).unwrap();
            let expected = if history == "middle" {
                (31, 32)
            } else {
                (11, 12)
            };
            assert_eq!((row.source, row.target), expected);
        }
        if reopen == 0 {
            vc.tag("committed", None)?;
            vc.save()?;
            #[cfg(not(target_arch = "wasm32"))]
            {
                drop(vc);
                vc = VersionControlDecorator::new(
                    transactions(named(&_data_files)?, named(&_log_files)?)?,
                    named(&_branch_files)?,
                    false,
                )?;
            }
            #[cfg(target_arch = "wasm32")]
            vc.recover()?;
        }
        assert_eq!(vc.try_get_tag("committed"), Some(expected_head));
    }
    Ok(())
}
macro_rules! contract {
    ($name:ident, $history:literal, $pending:literal) => {
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        fn $name() -> Result<()> {
            verify($history, $pending)
        }
    };
}
contract!(all_aborted_records_are_not_versions, "all", false);
contract!(trailing_abort_does_not_advance_initial_head, "tail", false);
contract!(interleaved_abort_is_not_reachable, "middle", false);
contract!(all_pending_records_are_not_versions, "all", true);
contract!(trailing_pending_does_not_advance_initial_head, "tail", true);
contract!(
    interleaved_recovered_pending_is_not_reachable,
    "middle",
    true
);

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn construction_rejects_a_transaction_with_a_live_owner() -> Result<()> {
    let (_data_files, data) = test_store::named()?;
    let (_log_files, log) = test_store::named()?;
    let (_branch_files, branches) = test_store::named()?;
    let mut tx = transactions(data, log)?;
    tx.begin_transaction()?;
    tx.create_and_update(21, 22)?;
    tx.save()?;
    assert!(VersionControlDecorator::new(tx, branches, false).is_err());
    #[cfg(not(target_arch = "wasm32"))]
    {
        let tx = transactions(named(&_data_files)?, named(&_log_files)?)?;
        let vc = VersionControlDecorator::new(tx, named(&_branch_files)?, false)?;
        assert_eq!(vc.current_sequence(), 0);
        assert!(vc.all().is_empty());
    }
    Ok(())
}

fn invalid_sidecar(pending: bool) -> Result<()> {
    let (_data_files, data) = test_store::named()?;
    let (_log_files, log) = test_store::named()?;
    let (_branch_files, mut branches) = test_store::named()?;
    let mut tx = transactions(data, log)?;
    let id = tx.create_and_update(11, 12)?;
    tx.begin_transaction()?;
    tx.update(id, 21, 22)?;
    let sequence = tx.last_logged_sequence();
    if !pending {
        tx.rollback()?;
    }
    tx.save()?;
    let (data, log) = tx.into_inner();
    for marker in [
        format!("__vc:branch:main:parent=:fork=0:head={sequence}"),
        format!("__vc:trans:{sequence}:branch=main"),
        "__vc:current=main".into(),
        format!("__vc:applied={sequence}"),
    ] {
        let row = branches.create(0, 0);
        branches.set_name(row, &marker)?;
    }
    branches.save()?;
    let tx = transactions(data, log)?;
    assert_eq!(tx.inner().get(id).unwrap().source, 11);
    match VersionControlDecorator::new(tx, branches, false) {
        Ok(_) => panic!("aborted sequence {sequence} was accepted from an existing sidecar"),
        Err(error) => assert!(error.to_string().contains(&sequence.to_string())),
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let data = named(&_data_files)?;
        assert_eq!(data.get(id).unwrap().source, 11);
        assert_eq!(named(&_branch_files)?.all().len(), 4);
    }
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_existing_sidecar_must_reject_rolled_back_versions() -> Result<()> {
    invalid_sidecar(false)
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_existing_sidecar_must_reject_recovered_pending_versions() -> Result<()> {
    invalid_sidecar(true)
}
