//! Validate the entire movement plan before changing rows or metadata.
use anyhow::Result;
use link_cli::{CommitMode, LogRetentionPolicy, NamedTypeLinks, TransactionsDecorator, Transition};
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;

fn encode(text: &str) -> String {
    text.encode_utf16()
        .map(|unit| format!("{unit:04X}"))
        .collect()
}
fn intent(
    from_branch: &str,
    from: i64,
    to_branch: &str,
    to: i64,
    undo: &[Transition],
    redo: &[Transition],
) -> String {
    let records = |values: &[Transition]| {
        values
            .iter()
            .map(|t| encode(&t.serialize()))
            .collect::<Vec<_>>()
            .join(",")
    };
    let body = format!(
        "1|{}|{from}|{}|{to}|{}|{}",
        encode(from_branch),
        encode(to_branch),
        records(undo),
        records(redo)
    );
    let hash = body.bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    format!("__vc:move:{body}|{hash:016x}")
}

fn contract(scenario: &str) -> Result<()> {
    let (data_files, data) = test_store::named()?;
    let (_log_files, log) = test_store::named()?;
    let (branch_files, mut branches) = test_store::named()?;
    let mut tx = TransactionsDecorator::new(
        data,
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )?;
    let id = tx.create_and_update(11, 12)?;
    let original = tx.last_logged_sequence();
    tx.update(id, 21, 22)?;
    let attempted = tx.last_logged_sequence();
    let record = *tx.log().last().unwrap();
    let mut payload = record;
    if scenario == "wrong-payload" {
        payload.after.source = 91;
    }
    if scenario == "wrong-decision" {
        payload.transaction_id += 1;
    }
    let from_branch = if scenario == "missing-from" {
        "missing"
    } else {
        "main"
    };
    let to_branch = if scenario == "missing-to" {
        "missing"
    } else {
        "original"
    };
    let from = if scenario == "unreachable-from" {
        i64::MAX
    } else {
        attempted
    };
    let to = if scenario == "unreachable-to" {
        i64::MAX
    } else {
        original
    };
    let undo = if scenario == "missing-undo" {
        vec![]
    } else {
        vec![payload]
    };
    let redo = if scenario == "extra-redo" {
        vec![record]
    } else {
        vec![]
    };
    let mut marker = intent(from_branch, from, to_branch, to, &undo, &redo);
    if scenario == "checksum" {
        let last = marker.pop().unwrap();
        marker.push(if last == '0' { '1' } else { '0' });
    }
    let parent = match scenario {
        "cyclic-parent" => "original",
        "missing-parent" => "missing",
        _ => "",
    };
    let selected = if scenario == "wrong-current" {
        "other"
    } else {
        "main"
    };
    let applied = match scenario {
        "wrong-applied" => 999,
        "impossible-publication" => original,
        _ => attempted,
    };
    let mut markers = vec![
        format!("__vc:branch:main:parent={parent}:fork=0:head={attempted}"),
        format!("__vc:branch:original:parent=main:fork={original}:head={original}"),
        format!("__vc:branch:other:parent=main:fork={original}:head={original}"),
        format!("__vc:current={selected}"),
        format!("__vc:applied={applied}"),
        marker,
    ];
    for sequence in 1..=attempted {
        markers.push(format!("__vc:trans:{sequence}:branch=main"));
    }
    match scenario {
        "overlap" => markers.push(format!(
            "__vc:pending:2|{:032x}|{attempted}|{attempted}|{}",
            record.transaction_id + 1,
            encode("main")
        )),
        "duplicate" => markers.push(intent("main", attempted, "main", original, &[record], &[])),
        "duplicate-current" => markers.push("__vc:current=original".into()),
        "duplicate-applied" => markers.push(format!("__vc:applied={original}")),
        "duplicate-branch" => markers.push(format!(
            "__vc:branch:original:parent=main:fork=0:head={original}"
        )),
        "duplicate-attribution" => markers.push(format!("__vc:trans:{attempted}:branch=original")),
        "invalid-branch" => markers.push("__vc:branch:broken".into()),
        "invalid-applied" => markers.push("__vc:applied=broken".into()),
        "invalid-attribution" => markers.push("__vc:trans:broken".into()),
        _ => {}
    }
    for marker in markers {
        let link = branches.try_create(0, 0)?;
        branches.set_name(link, &marker)?;
    }
    branches.save()?;
    if scenario == "evicted-record" {
        let log = tx.log_store_mut();
        let link = log
            .get_by_name(&format!("__transactions:transition:{}", record.serialize()))?
            .unwrap();
        log.remove_name(link)?;
        log.delete(link)?;
        log.save()?;
        tx.recover()?;
        assert!(!tx.log().iter().any(|t| t.sequence == attempted));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let before = (
        std::fs::read(branch_files.0.path())?,
        std::fs::read(branch_files.1.path())?,
    );
    let result = link_cli::VersionControlDecorator::new(tx, branches, false);
    if scenario == "valid" || scenario == "evicted-record" {
        let mut vc = result?;
        assert_eq!(vc.current_branch(), "original");
        assert_eq!(vc.current_sequence(), original);
        assert_eq!(vc.get(id).unwrap().source, 11);
        assert_eq!(
            vc.transactions().last_logged_sequence(),
            attempted,
            "{scenario}"
        );
        vc.update(id, 31, 32)?;
        assert_eq!(vc.current_sequence(), attempted + 1);
        vc.save()?;
    } else {
        assert!(result.is_err(), "accepted {scenario}");
        #[cfg(not(target_arch = "wasm32"))]
        {
            assert_eq!(
                before,
                (
                    std::fs::read(branch_files.0.path())?,
                    std::fs::read(branch_files.1.path())?
                ),
                "changed {scenario}"
            );
            let data = link_cli::NamedTypesDecorator::with_names_database_path(
                data_files.0.path(),
                data_files.1.path(),
                false,
            )?;
            assert_eq!(data.get(id).unwrap().source, 21, "changed rows {scenario}");
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (data_files, branch_files);
    Ok(())
}

macro_rules! case {
    ($name:ident, $scenario:literal) => {
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        fn $name() -> Result<()> {
            contract($scenario)
        }
    };
}
case!(missing_from, "missing-from");
case!(missing_to, "missing-to");
case!(unreachable_from, "unreachable-from");
case!(unreachable_to, "unreachable-to");
case!(missing_undo, "missing-undo");
case!(extra_redo, "extra-redo");
case!(wrong_payload, "wrong-payload");
case!(wrong_decision, "wrong-decision");
case!(wrong_current, "wrong-current");
case!(wrong_applied, "wrong-applied");
case!(impossible_publication, "impossible-publication");
case!(overlapping_intents, "overlap");
case!(duplicate_intents, "duplicate");
case!(invalid_checksum, "checksum");
case!(duplicate_current, "duplicate-current");
case!(duplicate_applied, "duplicate-applied");
case!(duplicate_branch, "duplicate-branch");
case!(duplicate_attribution, "duplicate-attribution");
case!(invalid_branch, "invalid-branch");
case!(invalid_applied, "invalid-applied");
case!(invalid_attribution, "invalid-attribution");
case!(cyclic_parent, "cyclic-parent");
case!(missing_parent, "missing-parent");
case!(evicted_record, "evicted-record");
case!(valid_record, "valid");
