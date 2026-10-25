//! Publication faults are portable; native cases also reopen real files.
use anyhow::Result;
use link_cli::{
    CommitMode, GenericVersionControlDecorator, LogRetentionPolicy, NamedTypeLinks,
    TransactionsDecorator,
};
#[cfg(not(target_arch = "wasm32"))]
use link_cli::{NamedTypesDecorator, VersionControlDecorator};
use std::cell::RefCell;
use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;

#[cfg(not(target_arch = "wasm32"))]
#[path = "common/version_metadata.rs"]
mod metadata_fault;
#[cfg(target_arch = "wasm32")]
use crate::version_metadata as metadata_fault;
use metadata_fault::{Fault, Metadata};

type VersionOwner = GenericVersionControlDecorator<Metadata>;

fn fences(vc: &mut VersionOwner, id: u32, attempted: i64) {
    assert!(vc.create(51, 52).is_err());
    assert!(vc.update(id, 51, 52).is_err());
    assert!(vc.delete(id).is_err());
    assert!(vc.set_name(id, "blocked").is_err());
    assert!(vc.remove_name(id).is_err());
    assert!(vc.begin_transaction().is_err());
    assert!(vc.branch("blocked", None).is_err());
    assert!(vc.switch_branch("main").is_err());
    assert!(vc.tag("blocked", None).is_err());
    assert!(vc.checkout(0).is_err());
    assert!(vc.recover().is_err());
    assert_eq!(vc.transactions().last_logged_sequence(), attempted);
}
fn cleanup(vc: &mut VersionOwner, entry: &str) -> Result<()> {
    match entry {
        "commit" => vc.commit(),
        "rollback" => vc.rollback(),
        "save" => vc.save(),
        _ => unreachable!(),
    }
}
fn phase_contract(phase: &str) -> Result<()> {
    for edge in ["before", "after"] {
        for automatic in [false, true] {
            for entry in ["commit", "rollback", "save"] {
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
                let original = vc.current_sequence();
                let topic = "topic|:parent=雪\0";
                vc.branch(topic, None)?;
                vc.switch_branch(topic)?;
                if !automatic {
                    vc.begin_transaction()?;
                    vc.update(id, 21, 22)?;
                }
                let point = format!("{phase}-{edge}");
                fault.borrow_mut().flushes = 0;
                fault.borrow_mut().point = Some(point.clone());
                let result = if automatic {
                    vc.update(id, 21, 22).map(|_| ())
                } else {
                    vc.commit()
                };
                assert!(
                    result.unwrap_err().to_string().contains(&point),
                    "{point}/{automatic}/{entry}"
                );
                assert!(fault.borrow().point.is_none());
                let attempted = vc.transactions().last_logged_sequence();
                assert_eq!(vc.get(id).unwrap().source, 21);
                fences(&mut vc, id, attempted);
                fault.borrow_mut().point = Some("flush-before".into());
                assert!(cleanup(&mut vc, entry)
                    .unwrap_err()
                    .to_string()
                    .contains("flush-before"));
                fences(&mut vc, id, attempted);
                cleanup(&mut vc, entry)?;
                let committed =
                    !matches!(phase, "intent-write" | "intent-flush") || entry != "rollback";
                assert_eq!(
                    vc.current_sequence(),
                    if committed { attempted } else { original }
                );
                assert_eq!(vc.get(id).unwrap().source, if committed { 21 } else { 11 });
                vc.checkout(original)?;
                assert_eq!(vc.get(id).unwrap().source, 11);
                if committed {
                    vc.checkout(attempted)?;
                } else {
                    assert!(vc.checkout(attempted).is_err());
                }
                vc.update(id, 31, 32)?;
                let following = vc.current_sequence();
                vc.save()?;
                vc.transactions_mut().recover()?;
                vc.recover()?;
                assert_eq!(vc.current_sequence(), following);
                assert_eq!(vc.get(id).unwrap().source, 31);
                drop(vc);
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let named = |files: &test_store::NamedFiles| {
                        NamedTypesDecorator::with_names_database_path(
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
                    assert_eq!(vc.current_branch(), topic);
                    assert_eq!(vc.current_sequence(), following);
                    assert_eq!(vc.get(id).unwrap().source, 31);
                }
                #[cfg(target_arch = "wasm32")]
                let _ = (data_files, log_files, branch_files);
            }
        }
    }
    Ok(())
}
macro_rules! contract {
    ($name:ident, $phase:literal) => {
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        fn $name() -> Result<()> {
            phase_contract($phase)
        }
    };
}
contract!(intent_write_failures, "intent-write");
contract!(intent_flush_failures, "intent-flush");
contract!(transition_marker_failures, "transition");
contract!(branch_head_failures, "head");
contract!(applied_sequence_failures, "applied");
contract!(publication_flush_failures, "publication-flush");
contract!(intent_name_removal_failures, "intent-remove");
contract!(intent_row_deletion_failures, "intent-delete");
contract!(final_cleanup_flush_failures, "cleanup-flush");

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn shared_intent_vectors_are_recovered_or_rejected_without_guessing() -> Result<()> {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/version-publication-vectors.json"
    ))?;
    for valid in [true, false] {
        let rows = vectors[if valid { "intents" } else { "invalidIntents" }]
            .as_array()
            .unwrap();
        for row in rows {
            let marker = if valid {
                row["marker"].as_str().unwrap()
            } else {
                row.as_str().unwrap()
            };
            if !marker.starts_with("__vc:pending:") {
                continue;
            } // Unrelated metadata is ignored.
            let branch = if valid {
                row["branch"].as_str().unwrap()
            } else {
                "main"
            };
            let (_data_files, data) = test_store::named()?;
            let (_log_files, log) = test_store::named()?;
            let (branch_files, mut branches) = test_store::named()?;
            let mut write = |name: &str| -> Result<()> {
                let id = branches.create(0, 0);
                branches.set_name(id, name)?;
                Ok(())
            };
            write("__vc:branch:2|006D00610069006E|-|0|0")?;
            if branch != "main" {
                let encoded: String = branch
                    .encode_utf16()
                    .map(|unit| format!("{unit:04X}"))
                    .collect();
                write(&format!("__vc:branch:2|{encoded}|006D00610069006E|0|0"))?;
            }
            write(&format!("__vc:current={branch}"))?;
            write("__vc:applied=0")?;
            write(marker)?;
            branches.save()?;
            #[cfg(not(target_arch = "wasm32"))]
            let before = (
                std::fs::read(branch_files.0.path())?,
                std::fs::read(branch_files.1.path())?,
            );
            let tx = TransactionsDecorator::new(
                data,
                log,
                LogRetentionPolicy::Infinite,
                CommitMode::Sync,
                false,
            )?;
            let result = GenericVersionControlDecorator::new(
                tx,
                Metadata {
                    inner: branches,
                    fault: Rc::new(RefCell::new(Fault::default())),
                    intent: 0,
                },
                true,
            );
            if valid {
                let mut vc = result?;
                assert_eq!(vc.current_branch(), branch);
                assert_eq!(vc.current_sequence(), 0);
                assert!(vc.all().is_empty());
                vc.branch("following", None)?;
                vc.save()?;
                vc.recover()?;
            } else {
                assert!(result.is_err(), "accepted malformed intent: {marker}");
                #[cfg(not(target_arch = "wasm32"))]
                assert_eq!(
                    before,
                    (
                        std::fs::read(branch_files.0.path())?,
                        std::fs::read(branch_files.1.path())?
                    )
                );
            }
            #[cfg(target_arch = "wasm32")]
            let _ = branch_files;
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn actual_reopening_retries_every_interrupted_recovery_phase() -> Result<()> {
    let named = |files: &test_store::NamedFiles| {
        NamedTypesDecorator::with_names_database_path(files.0.path(), files.1.path(), false)
    };
    for phase in [
        "transition",
        "head",
        "applied",
        "publication-flush",
        "intent-remove",
        "intent-delete",
        "cleanup-flush",
    ] {
        for edge in ["before", "after"] {
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
                false,
            )?;
            let id = vc.create_and_update(11, 12)?;
            let original = vc.current_sequence();
            let topic = "topic|:parent=雪\0";
            vc.branch(topic, None)?;
            vc.switch_branch(topic)?;
            vc.begin_transaction()?;
            vc.update(id, 21, 22)?;
            fault.borrow_mut().flushes = 0;
            fault.borrow_mut().point = Some("publication-flush-before".into());
            assert!(vc.commit().is_err());
            let attempted = vc.transactions().last_logged_sequence();
            drop(vc);
            let tx = TransactionsDecorator::new(
                named(&data_files)?,
                named(&log_files)?,
                LogRetentionPolicy::Infinite,
                CommitMode::Sync,
                false,
            )?;
            let point = format!("{phase}-{edge}");
            let fault = Rc::new(RefCell::new(Fault {
                point: Some(point.clone()),
                flushes: 1,
                on_check: None,
            }));
            let result = GenericVersionControlDecorator::new(
                tx,
                Metadata {
                    inner: named(&branch_files)?,
                    fault: fault.clone(),
                    intent: 0,
                },
                true,
            );
            assert!(result
                .err()
                .expect("recovery fault was not reached")
                .to_string()
                .contains(&point));
            assert!(fault.borrow().point.is_none());
            let mut following = 0;
            for reopen in 0..2 {
                let tx = TransactionsDecorator::new(
                    named(&data_files)?,
                    named(&log_files)?,
                    LogRetentionPolicy::Infinite,
                    CommitMode::Sync,
                    false,
                )?;
                let mut vc = VersionControlDecorator::new(tx, named(&branch_files)?, false)?;
                assert_eq!(vc.current_branch(), topic);
                assert_eq!(
                    vc.current_sequence(),
                    if reopen == 0 { attempted } else { following }
                );
                assert_eq!(
                    vc.get(id).unwrap().source,
                    if reopen == 0 { 21 } else { 31 }
                );
                if reopen == 0 {
                    vc.checkout(original)?;
                    assert_eq!(vc.get(id).unwrap().source, 11);
                    vc.checkout(attempted)?;
                    assert_eq!(vc.get(id).unwrap().source, 21);
                    vc.update(id, 31, 32)?;
                    following = vc.current_sequence();
                }
                vc.save()?;
            }
        }
    }
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn contradictory_intents_preserve_metadata_and_committed_rows() -> Result<()> {
    for scenario in [
        "unknown-branch",
        "wrong-current",
        "head-ahead",
        "applied-ahead",
        "foreign-record",
        "missing-record",
        "foreign-attribution",
        "multiple-intents",
        "conflicting-decisions",
    ] {
        let (_data_files, data) = test_store::named()?;
        let (_log_files, log) = test_store::named()?;
        let (branch_files, mut branches) = test_store::named()?;
        let mut tx = TransactionsDecorator::new(
            data,
            log,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )?;
        tx.begin_transaction()?;
        let id = tx.create_and_update(11, 12)?;
        tx.commit()?;
        let original = tx.last_logged_sequence();
        tx.begin_transaction()?;
        tx.update(id, 21, 22)?;
        tx.commit()?;
        let attempted = tx.last_logged_sequence();
        let transaction = tx.log().last().unwrap().transaction_id;
        let encode = |name: &str| -> String {
            name.encode_utf16()
                .map(|unit| format!("{unit:04X}"))
                .collect()
        };
        let topic = "topic|:parent=雪\0";
        let main_encoded = encode("main");
        let topic_encoded = encode(topic);
        let mut markers = vec![
            format!("__vc:branch:2|{main_encoded}|-|0|{original}"),
            format!(
                "__vc:branch:2|{topic_encoded}|{main_encoded}|{original}|{}",
                if scenario == "head-ahead" {
                    attempted + 1
                } else {
                    attempted
                }
            ),
            format!(
                "__vc:current={}",
                if scenario == "wrong-current" {
                    "main"
                } else {
                    topic
                }
            ),
            format!(
                "__vc:applied={}",
                if scenario == "applied-ahead" {
                    attempted + 1
                } else {
                    attempted
                }
            ),
        ];
        for sequence in 1..=attempted {
            let branch = if sequence <= original || scenario == "foreign-attribution" {
                "main"
            } else {
                topic
            };
            markers.push(format!("__vc:trans:{sequence}:branch={branch}"));
        }
        let before = if scenario == "foreign-record" {
            0
        } else {
            original
        };
        let after = if scenario == "missing-record" {
            attempted + 1
        } else {
            attempted
        };
        let branch = if scenario == "unknown-branch" {
            encode("unknown")
        } else {
            topic_encoded
        };
        markers.push(format!(
            "__vc:pending:2|{transaction:032x}|{before}|{after}|{branch}"
        ));
        if scenario == "multiple-intents" {
            markers.push(format!(
                "__vc:pending:2|{:032x}|{before}|{after}|{branch}",
                transaction + 1
            ));
        }
        if scenario == "conflicting-decisions" {
            let log = tx.log_store_mut();
            let marker = log.create(0, 0);
            log.set_name(
                marker,
                &format!("__transactions:rollback:{transaction:032x}"),
            )?;
            log.save()?;
            tx.recover()?;
        }
        for marker in markers {
            let id = branches.create(0, 0);
            branches.set_name(id, &marker)?;
        }
        branches.save()?;
        assert_eq!(tx.get(id).unwrap().source, 21);
        #[cfg(not(target_arch = "wasm32"))]
        let before = (
            std::fs::read(branch_files.0.path())?,
            std::fs::read(branch_files.1.path())?,
        );
        assert!(
            GenericVersionControlDecorator::new(
                tx,
                Metadata {
                    inner: branches,
                    fault: Rc::new(RefCell::new(Fault::default())),
                    intent: 0
                },
                false
            )
            .is_err(),
            "accepted {scenario}"
        );
        #[cfg(not(target_arch = "wasm32"))]
        assert_eq!(
            before,
            (
                std::fs::read(branch_files.0.path())?,
                std::fs::read(branch_files.1.path())?
            )
        );
        #[cfg(not(target_arch = "wasm32"))]
        {
            let data = NamedTypesDecorator::with_names_database_path(
                _data_files.0.path(),
                _data_files.1.path(),
                false,
            )?;
            assert_eq!(data.get(id).unwrap().source, 21);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = branch_files;
    }
    Ok(())
}
