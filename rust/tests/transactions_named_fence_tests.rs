//! Named writes participate in the foreground owner's failure fence.
use anyhow::Result;
use link_cli::{
    CommitMode, LogRetentionPolicy, NamedTypeLinks, NamedTypesDecorator, TransactionsDecorator,
    VersionControlDecorator,
};
use std::path::Path;

fn open(directory: &Path) -> Result<TransactionsDecorator> {
    Ok(TransactionsDecorator::new(
        NamedTypesDecorator::new(directory.join("data/data.links"), false)?,
        NamedTypesDecorator::new(directory.join("log.links"), false)?,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )?)
}

fn fail_checkpoint(directory: &Path, complete: impl FnOnce() -> Result<()>) -> Result<()> {
    std::fs::rename(directory.join("data"), directory.join("moved"))?;
    let outcome = complete();
    std::fs::rename(directory.join("moved"), directory.join("data"))?;
    assert!(
        outcome.is_err(),
        "moving the data directory must fail persistence"
    );
    Ok(())
}

fn rejected_named_write(owner: &mut impl NamedTypeLinks, id: u32, remove: bool) -> Result<()> {
    let outcome = if remove {
        owner.remove_name(id)
    } else {
        owner.set_name(id, "changed").map(|_| ())
    };
    assert!(outcome.is_err(), "failed cleanup must fence named writes");
    assert_eq!(owner.get_name(id)?.as_deref(), Some("original"));
    assert_eq!(owner.get_by_name("original")?, Some(id));
    Ok(())
}

fn verify_transaction(committed: bool, remove: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::create_dir(directory.path().join("data"))?;
    let mut tx = open(directory.path())?;
    let id = tx.create(11, 12)?;
    NamedTypeLinks::set_name(&mut tx, id, "original")?;
    tx.save()?;
    tx.begin_transaction()?;
    tx.update(id, 21, 22)?;
    fail_checkpoint(directory.path(), || {
        if committed {
            tx.commit()?;
        } else {
            tx.rollback()?;
        }
        Ok(())
    })?;
    let sequence = tx.last_logged_sequence();
    rejected_named_write(&mut tx, id, remove)?;
    assert_eq!(tx.last_logged_sequence(), sequence);
    tx.rollback()?;
    let expected_source = if committed { 21 } else { 11 };
    assert_eq!(tx.get(id).unwrap().source, expected_source);
    NamedTypeLinks::set_name(&mut tx, id, "changed")?;
    NamedTypeLinks::remove_name(&mut tx, id)?;
    assert_eq!(NamedTypeLinks::get_name(&mut tx, id)?, None);
    NamedTypeLinks::set_name(&mut tx, id, "original")?;
    tx.update(id, 31, 32)?;
    tx.save()?;
    drop(tx);
    let mut recovered = open(directory.path())?;
    assert_eq!(
        (
            recovered.get(id).unwrap().source,
            recovered.get(id).unwrap().target
        ),
        (31, 32)
    );
    assert_eq!(
        NamedTypeLinks::get_name(&mut recovered, id)?.as_deref(),
        Some("original")
    );
    Ok(())
}

fn verify_version_rollback(remove: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::create_dir(directory.path().join("data"))?;
    let mut vc = VersionControlDecorator::new(
        open(directory.path())?,
        NamedTypesDecorator::new(directory.path().join("branches.links"), false)?,
        false,
    )?;
    let id = vc.create(11, 12)?;
    NamedTypeLinks::set_name(&mut vc, id, "original")?;
    vc.save()?;
    vc.begin_transaction()?;
    vc.update(id, 21, 22)?;
    fail_checkpoint(directory.path(), || vc.rollback())?;
    let sequence = vc.transactions().last_logged_sequence();
    rejected_named_write(&mut vc, id, remove)?;
    assert_eq!(vc.transactions().last_logged_sequence(), sequence);
    vc.rollback()?;
    NamedTypeLinks::set_name(&mut vc, id, "changed")?;
    NamedTypeLinks::remove_name(&mut vc, id)?;
    assert_eq!(NamedTypeLinks::get_name(&mut vc, id)?, None);
    vc.save()?;
    Ok(())
}

macro_rules! transaction_case {
    ($name:ident, $committed:expr, $remove:expr) => {
        #[test]
        fn $name() -> Result<()> {
            verify_transaction($committed, $remove)
        }
    };
}
transaction_case!(committed_checkpoint_fences_set_name, true, false);
transaction_case!(committed_checkpoint_fences_remove_name, true, true);
transaction_case!(failed_rollback_fences_set_name, false, false);
transaction_case!(failed_rollback_fences_remove_name, false, true);
#[test]
fn failed_version_rollback_fences_set_name() -> Result<()> {
    verify_version_rollback(false)
}
#[test]
fn failed_version_rollback_fences_remove_name() -> Result<()> {
    verify_version_rollback(true)
}
