//! A journal imported at a fixed JS clock cannot donate an old owner ID.
#![cfg(target_arch = "wasm32")]
use anyhow::Result;
use link_cli::{
    CommitMode, LinkStorage, LogRetentionPolicy, NamedTypeLinks, NamedTypesDecorator,
    TransactionsDecorator,
};

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = "
let originalIdentityDateNow;
export function freeze_identity_clock() {
    originalIdentityDateNow = Date.now;
    Date.now = () => 1730000000000;
}
export function restore_identity_clock() { Date.now = originalIdentityDateNow; }
")]
extern "C" {
    fn freeze_identity_clock();
    fn restore_identity_clock();
}

fn named() -> NamedTypesDecorator {
    NamedTypesDecorator::from_link_storages(
        LinkStorage::in_memory(false),
        LinkStorage::in_memory(false),
    )
}
fn tx(data: NamedTypesDecorator, log: NamedTypesDecorator) -> Result<TransactionsDecorator> {
    Ok(TransactionsDecorator::new(
        data,
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )?)
}

fn contract(aborted: bool, automatic: bool) -> Result<()> {
    freeze_identity_clock();
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            restore_identity_clock();
        }
    }
    let restore = Restore;
    let mut probe = tx(named(), named())?;
    let first = probe.begin_transaction()?;
    probe.rollback()?;
    // Synchronous WASM tests consume the next counter without other JS turns.
    let collision = first.id + 1;
    let mut log = named();
    let marker = log.try_create(0, 0)?;
    log.set_name(
        marker,
        &format!(
            "__transactions:{}:{collision:032x}",
            if aborted { "rollback" } else { "commit" }
        ),
    )?;
    let mut data = named();
    let row = data.try_create(11, 12)?;
    let mut owner = tx(data, log)?;
    let allocated = if automatic {
        owner.update(row, 21, 22)?;
        owner.log().last().unwrap().transaction_id
    } else {
        let handle = owner.begin_transaction()?;
        owner.update(row, 21, 22)?;
        owner.rollback()?;
        handle.id
    };
    owner.recover()?;
    let recovered = owner.get(row).unwrap().source;
    owner.update(row, 31, 32)?;
    owner.recover()?;
    let following = owner.get(row).unwrap().source;
    // Restore before assertions: a WASM panic need not run Rust destructors.
    drop(restore);
    assert_ne!(
        allocated, collision,
        "A recovered journal decision identified a new owner."
    );
    assert_eq!(recovered, if automatic { 21 } else { 11 });
    assert_eq!(following, 31);
    Ok(())
}
#[wasm_bindgen_test::wasm_bindgen_test]
fn committed_id_is_not_reused_by_explicit_wasm_owner() -> Result<()> {
    contract(false, false)
}
#[wasm_bindgen_test::wasm_bindgen_test]
fn aborted_id_is_not_reused_by_explicit_wasm_owner() -> Result<()> {
    contract(true, false)
}
#[wasm_bindgen_test::wasm_bindgen_test]
fn committed_id_is_not_reused_by_automatic_wasm_owner() -> Result<()> {
    contract(false, true)
}
#[wasm_bindgen_test::wasm_bindgen_test]
fn aborted_id_is_not_reused_by_automatic_wasm_owner() -> Result<()> {
    contract(true, true)
}
