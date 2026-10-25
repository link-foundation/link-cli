//! Movement owns replay until its rows and selected history position are durable.
use anyhow::Result;
use link_cli::{
    CommitMode, GenericVersionControlDecorator, LogRetentionPolicy, NamedTypeLinks,
    TransactionsDecorator,
};
use std::{cell::RefCell, rc::Rc};
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/version_metadata.rs"]
mod metadata_fault;
#[cfg(target_arch = "wasm32")]
use crate::version_metadata as metadata_fault;
use metadata_fault::{Fault, Metadata};
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;

fn contract(switch: bool, point: &str) -> Result<()> {
    let (data_files, data) = test_store::named()?;
    let (log_files, log) = test_store::named()?;
    let (branch_files, branches) = test_store::named()?;
    let fault = Rc::new(RefCell::new(Fault::default()));
    let tx = TransactionsDecorator::new(
        data,
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )?;
    let mut vc = GenericVersionControlDecorator::new(
        tx,
        Metadata {
            inner: branches,
            fault: fault.clone(),
            intent: 0,
        },
        true,
    )?;
    let id = vc.create_and_update(11, 12)?;
    vc.set_name(id, "original 雪|\0\n")?;
    let original = vc.current_sequence();
    vc.branch("original", None)?;
    vc.update(id, 21, 22)?;
    vc.set_name(id, "changed 雪|\0\n")?;
    let attempted = vc.current_sequence();
    let transition = *vc.transactions().log().last().unwrap();
    let heads = vc.list_branches();
    fault.borrow_mut().flushes = 0;
    fault.borrow_mut().point = Some(point.into());
    let outcome = if switch {
        vc.switch_branch("original")
    } else {
        vc.checkout(original)
    };
    assert!(
        outcome.is_err(),
        "movement omitted its persistence barrier: {point}"
    );
    assert!(fault.borrow().point.is_none());
    let fences = |vc: &mut GenericVersionControlDecorator<Metadata>| {
        assert!(
            vc.update(id, 51, 52).is_err(),
            "failed movement admitted a write"
        );
        assert!(vc.create(51, 52).is_err());
        assert!(vc.delete(id).is_err());
        assert!(vc.set_name(id, "blocked").is_err());
        assert!(vc.remove_name(id).is_err());
        assert!(vc.begin_transaction().is_err());
        assert!(vc.branch("blocked", None).is_err());
        assert!(vc.tag("blocked", None).is_err());
        assert!(vc.checkout(0).is_err());
        assert!(vc.switch_branch("main").is_err());
        assert!(vc.recover().is_err());
        let tx = vc.transactions_mut();
        assert!(tx.create(51, 52).is_err());
        assert!(tx.begin_transaction().is_err());
        assert!(tx.recover().is_err());
        assert!(tx.apply_transition(&transition).is_err());
        assert!(tx.revert_transition(&transition).is_err());
        assert_eq!(tx.last_logged_sequence(), attempted);
    };
    fences(&mut vc);
    fault.borrow_mut().point = Some("flush-before".into());
    assert!(vc.save().is_err());
    fences(&mut vc);
    vc.save()?;
    assert!(fault.borrow().point.is_none());
    let selected = if switch { "original" } else { "main" };
    assert_eq!(vc.current_branch(), selected);
    assert_eq!(vc.current_sequence(), original);
    assert_eq!(vc.list_branches(), heads);
    assert_eq!(vc.get(id).unwrap().source, 11);
    assert_eq!(vc.get_name(id)?.as_deref(), Some("original 雪|\0\n"));
    vc.update(id, 31, 32)?;
    let following = vc.current_sequence();
    assert_eq!(following, attempted + 1);
    vc.save()?;
    drop(vc);
    #[cfg(not(target_arch = "wasm32"))]
    for _ in 0..2 {
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
        let mut reopened =
            link_cli::VersionControlDecorator::new(tx, named(&branch_files)?, false)?;
        assert_eq!(reopened.current_branch(), selected);
        assert_eq!(reopened.current_sequence(), following);
        assert_eq!(reopened.get(id).unwrap().source, 31);
        assert_eq!(reopened.get_name(id)?.as_deref(), Some("original 雪|\0\n"));
        reopened.save()?;
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (data_files, log_files, branch_files);
    Ok(())
}

macro_rules! case {
    ($name:ident, $switch:expr, $point:expr) => {
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        fn $name() -> Result<()> {
            contract($switch, $point)
        }
    };
}
case!(checkout_applied_before, false, "applied-before");
case!(checkout_applied_after, false, "applied-after");
case!(checkout_flush_before, false, "flush-before");
case!(switch_applied_before, true, "applied-before");
case!(switch_applied_after, true, "applied-after");
case!(switch_flush_before, true, "flush-before");

macro_rules! phases {
    ($name:ident, $phase:literal) => {
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        fn $name() -> Result<()> {
            for switch in [false, true] {
                for edge in ["before", "after"] {
                    contract(switch, &format!("{}-{edge}", $phase))?;
                }
            }
            Ok(())
        }
    };
}
phases!(movement_intent_write_faults, "intent-write");
phases!(movement_intent_flush_faults, "intent-flush");
phases!(movement_current_faults, "current");
phases!(movement_publication_flush_faults, "publication-flush");
phases!(movement_intent_remove_faults, "intent-remove");
phases!(movement_intent_delete_faults, "intent-delete");
phases!(movement_cleanup_flush_faults, "cleanup-flush");

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn shared_wire_vectors_are_validated_during_portable_recovery() -> Result<()> {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/version-movement-vectors.json"
    ))?;
    for (valid, field) in [
        (true, "movements"),
        (false, "invalidMovements"),
        (false, "nonUnicodeMovements"),
    ] {
        for row in vectors[field].as_array().unwrap() {
            let (data_files, data) = test_store::named()?;
            let (log_files, log) = test_store::named()?;
            let (branch_files, mut branches) = test_store::named()?;
            let marker = if valid {
                row["marker"].as_str().unwrap()
            } else {
                row.as_str().unwrap()
            };
            // Unknown prefixes are deliberately ignored by sidecar recovery.
            if !marker.starts_with("__vc:move:") {
                continue;
            }
            let id = branches.try_create(0, 0)?;
            branches.set_name(id, marker)?;
            let tx = TransactionsDecorator::new(
                data,
                log,
                LogRetentionPolicy::Infinite,
                CommitMode::Sync,
                false,
            )?;
            let result = link_cli::VersionControlDecorator::new(tx, branches, false);
            let error = match result {
                Ok(_) => panic!("unreachable movement accepted: {marker}"),
                Err(error) => error.to_string(),
            };
            if valid {
                assert!(
                    error.contains("Ambiguous version movement metadata"),
                    "{error}: {marker}"
                );
            } else {
                assert!(
                    error.contains("Invalid version movement intent"),
                    "{error}: {marker}"
                );
            }
            let _ = (data_files, log_files, branch_files);
        }
    }
    Ok(())
}
