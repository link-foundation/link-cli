//! A retained branch path must never silently skip unavailable transitions.
use anyhow::Result;
use link_cli::{CommitMode, LogRetentionPolicy, TransactionsDecorator, VersionControlDecorator};
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;

fn verify(entry: &str) -> Result<()> {
    let (data_files, data) = test_store::named()?;
    let (log_files, log) = test_store::named()?;
    let (branch_files, branches) = test_store::named()?;
    let tx = TransactionsDecorator::new(
        data,
        log,
        LogRetentionPolicy::Sized { max_transitions: 1 },
        CommitMode::Sync,
        false,
    )?;
    let mut vc = VersionControlDecorator::new(tx, branches, false)?;
    let id = vc.create_and_update(11, 12)?;
    let original = vc.current_sequence();
    vc.branch("empty", Some(0))?;
    vc.branch("safe", Some(original))?;
    vc.update(id, 21, 22)?;
    let current = vc.current_sequence();
    assert_eq!(vc.transactions().log().len(), 1);
    if entry == "redo" {
        vc.switch_branch("safe")?;
        vc.update(id, 31, 32)?;
    }
    let selected = vc.current_branch().to_string();
    let head = vc.current_sequence();
    let source = vc.get(id).unwrap().source;
    let branches = vc.list_branches();
    let outcome = match entry {
        "switch" => vc.switch_branch("empty"),
        "redo" => vc.switch_branch("main"),
        _ => vc.checkout(0),
    };
    assert!(outcome.is_err(), "incomplete history was silently accepted");
    assert_eq!(vc.current_branch(), selected);
    assert_eq!(vc.current_sequence(), head);
    assert_eq!(vc.list_branches(), branches);
    assert_eq!(
        vc.get(id).unwrap().source,
        source,
        "failure partially changed data"
    );
    if entry != "redo" {
        // Missing shared-prefix records are not needed for this available suffix.
        vc.switch_branch("safe")?;
        assert_eq!(vc.current_sequence(), original);
        assert_eq!(vc.get(id).unwrap().source, 11);
        vc.switch_branch("main")?;
        assert_eq!(vc.current_sequence(), current);
        assert_eq!(vc.get(id).unwrap().source, 21);
        vc.checkout(current)?;
        vc.update(id, 31, 32)?;
        let following = vc.current_sequence();
        assert!(vc.checkout(original).is_err());
        assert_eq!(vc.current_sequence(), following);
        assert_eq!(vc.get(id).unwrap().source, 31);
    }
    let following = vc.current_sequence();
    vc.save()?;
    drop(vc);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let named = |files: &test_store::NamedFiles| {
            link_cli::NamedTypesDecorator::with_names_database_path(
                files.0.path(),
                files.1.path(),
                false,
            )
        };
        let tx = TransactionsDecorator::new(
            named(&data_files)?,
            named(&log_files)?,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )?;
        let vc = VersionControlDecorator::new(tx, named(&branch_files)?, false)?;
        assert_eq!(vc.current_branch(), selected);
        assert_eq!(vc.current_sequence(), following);
        assert_eq!(vc.get(id).unwrap().source, 31);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (data_files, log_files, branch_files, following);
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checkout_rejects_missing_undo_before_mutation() -> Result<()> {
    verify("checkout")
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn branch_switch_rejects_missing_undo_before_mutation() -> Result<()> {
    verify("switch")
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn branch_switch_rejects_missing_redo_before_undoing_current_branch() -> Result<()> {
    verify("redo")
}
