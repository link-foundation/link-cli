//! Later committed states must survive an earlier applied-marker gap.
use anyhow::Result;
use link_cli::{
    transactions::DoubletLink, CommitMode, LinksStorage, LogRetentionPolicy, TransactionsDecorator,
    Transition, TransitionKind, TransitionLogStore,
};
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;

fn verify(operation: &str) -> Result<()> {
    let (_data_files, mut data) = test_store::named()?;
    let (_log_files, mut log) = test_store::named()?;
    let id = data.create_link(31, 32)?;
    if operation == "delete" {
        data.delete_link(id)?;
    }
    let state = |source, target| DoubletLink::new(id, source, target);
    let empty = DoubletLink::empty();
    let (older_kind, older_after) = if operation == "recreate" {
        (TransitionKind::Delete, empty)
    } else {
        (TransitionKind::Update, state(21, 22))
    };
    let (newer_kind, newer_before, newer_after) = match operation {
        "delete" => (TransitionKind::Delete, state(21, 22), empty),
        "recreate" => (TransitionKind::Create, empty, state(31, 32)),
        _ => (TransitionKind::Update, state(21, 22), state(31, 32)),
    };
    let changes = [
        (TransitionKind::Create, empty, state(11, 12)),
        (older_kind, state(11, 12), older_after),
        (newer_kind, newer_before, newer_after),
    ];
    for (index, (kind, before, after)) in changes.into_iter().enumerate() {
        let sequence = index as i64 + 1;
        let transition = Transition {
            transaction_id: sequence as u128,
            sequence,
            timestamp_ms: 0,
            kind,
            before,
            after,
        };
        log.append_log_entry(&format!(
            "__transactions:transition:{}",
            transition.serialize()
        ))?;
        log.append_log_entry(&format!(
            "__transactions:commit:{:032x}",
            transition.transaction_id
        ))?;
        if sequence != 2 {
            log.append_log_entry(&format!("__transactions:applied:{sequence}"))?;
        }
    }
    let mut recovered = TransactionsDecorator::new(
        data,
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )?;
    for _ in 0..2 {
        if operation == "delete" {
            assert!(!recovered.exists(id));
        } else {
            let row = recovered
                .get(id)
                .expect("latest committed row must survive replay");
            assert_eq!((row.source, row.target), (31, 32));
        }
        assert_eq!(recovered.applied_sequence(), 3);
        recovered.recover()?;
    }
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn applied_update_survives_older_gap() -> Result<()> {
    verify("update")
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn applied_delete_survives_older_gap() -> Result<()> {
    verify("delete")
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn applied_recreation_survives_older_gap() -> Result<()> {
    verify("recreate")
}
