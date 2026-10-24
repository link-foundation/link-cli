//! Finite sparse histories must not manufacture reachable sequence numbers.
use anyhow::Result;
use link_cli::{
    CommitMode, DoubletLink, LogRetentionPolicy, NamedTypesDecorator, TransactionsDecorator,
    Transition, TransitionKind, TransitionLogStore, VersionControlDecorator,
};
use std::path::Path;

fn open_transactions(directory: &Path) -> Result<TransactionsDecorator> {
    Ok(TransactionsDecorator::new(
        NamedTypesDecorator::new(directory.join("data.links"), false)?,
        NamedTypesDecorator::new(directory.join("log.links"), false)?,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )?)
}

fn seed_sequence(directory: &Path, baseline: u32, sequence: i64) -> Result<()> {
    let state = DoubletLink::new(baseline, 11, 12);
    let transition = Transition {
        transaction_id: 1,
        sequence,
        timestamp_ms: 0,
        kind: TransitionKind::Update,
        before: state,
        after: state,
    };
    let mut log = NamedTypesDecorator::new(directory.join("log.links"), false)?;
    for marker in [
        format!("__transactions:transition:{}", transition.serialize()),
        format!("__transactions:commit:{:032x}", 1),
        format!("__transactions:applied:{sequence}"),
    ] {
        log.append_log_entry(&marker)?;
    }
    log.flush_log()?;
    Ok(())
}

#[test]
fn sparse_initial_history_rejects_unrecorded_gap_checkout() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut tx = open_transactions(directory.path())?;
    let baseline = tx.create_and_update(11, 12)?;
    tx.save()?;
    drop(tx);
    // Deliberately finite even against the old numeric-range implementation.
    // Three real records, with a large gap, suffice to expose phantom history.
    seed_sequence(directory.path(), baseline, 100)?;
    let mut vc = VersionControlDecorator::new(
        open_transactions(directory.path())?,
        NamedTypesDecorator::new(directory.path().join("branches.links"), false)?,
        false,
    )?;
    assert_eq!(vc.transactions().log().len(), 3);
    assert_eq!(vc.current_sequence(), 100);
    assert!(
        vc.checkout(3).is_err(),
        "an unrecorded gap is not a version"
    );
    assert!(vc.branch("gap", Some(3)).is_err());
    vc.checkout(2)?;
    assert_eq!(vc.all().len(), 1);
    assert_eq!(vc.get(baseline).unwrap().source, 11);
    vc.checkout(100)?;
    vc.tag("last", None)?;
    vc.save()?;
    drop(vc);
    let mut recovered = VersionControlDecorator::new(
        open_transactions(directory.path())?,
        NamedTypesDecorator::new(directory.path().join("branches.links"), false)?,
        false,
    )?;
    assert_eq!(recovered.current_sequence(), 100);
    assert_eq!(recovered.try_get_tag("last"), Some(100));
    assert!(recovered.checkout(3).is_err());
    assert_eq!(recovered.all().len(), 1);
    Ok(())
}

fn verify_final_sequence(explicit: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut vc = VersionControlDecorator::new(
        open_transactions(directory.path())?,
        NamedTypesDecorator::new(directory.path().join("branches.links"), false)?,
        false,
    )?;
    let baseline = vc.create_and_update(11, 12)?;
    vc.save()?;
    drop(vc);
    seed_sequence(directory.path(), baseline, i64::MAX - 1)?;
    let mut vc = VersionControlDecorator::new(
        open_transactions(directory.path())?,
        NamedTypesDecorator::new(directory.path().join("branches.links"), false)?,
        false,
    )?;
    if explicit {
        vc.begin_transaction()?;
    }
    let created = vc.create(0, 0)?;
    if explicit {
        vc.commit()?;
    }
    assert_eq!(vc.current_sequence(), i64::MAX);
    assert_eq!(vc.list_branches()[0].head, i64::MAX);
    vc.tag("last", None)?;
    assert!(vc.create(51, 52).is_err());
    assert_eq!(vc.all().len(), 2);
    vc.save()?;
    drop(vc);
    let mut recovered = VersionControlDecorator::new(
        open_transactions(directory.path())?,
        NamedTypesDecorator::new(directory.path().join("branches.links"), false)?,
        false,
    )?;
    assert_eq!(recovered.current_sequence(), i64::MAX);
    assert_eq!(recovered.try_get_tag("last"), Some(i64::MAX));
    assert!(recovered.exists(baseline));
    assert!(recovered.exists(created));
    recovered.checkout(2)?;
    assert_eq!(recovered.all().len(), 1);
    assert!(!recovered.exists(created));
    recovered.checkout(i64::MAX)?;
    assert_eq!(recovered.all().len(), 2);
    assert!(recovered.exists(created));
    assert!(recovered.create(61, 62).is_err());
    Ok(())
}

#[test]
fn final_automatic_sequence_preserves_versions() -> Result<()> {
    verify_final_sequence(false)
}

#[test]
fn final_explicit_sequence_preserves_versions() -> Result<()> {
    verify_final_sequence(true)
}
