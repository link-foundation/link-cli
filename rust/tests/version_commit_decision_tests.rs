//! Uncertain commit delivery keeps its version owner until publication finishes.
use anyhow::Result;
use link_cli::{
    CommitMode, LogRetentionPolicy, NamedTypeLinks, NamedTypesDecorator, TransactionsDecorator,
    TransitionLogStore, VersionControlDecorator,
};
use std::path::Path;

fn open(directory: &Path) -> Result<VersionControlDecorator> {
    VersionControlDecorator::new(
        TransactionsDecorator::new(
            NamedTypesDecorator::new(directory.join("data/data.links"), false)?,
            NamedTypesDecorator::new(directory.join("log/log.links"), false)?,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )?,
        NamedTypesDecorator::new(directory.join("branches.links"), false)?,
        false,
    )
}

struct FailedCheckpoint {
    directory: tempfile::TempDir,
    vc: VersionControlDecorator,
    baseline: u32,
    created: u32,
    expected: i64,
}

fn uncertain_decision_failure() -> Result<FailedCheckpoint> {
    let directory = tempfile::tempdir()?;
    std::fs::create_dir(directory.path().join("data"))?;
    std::fs::create_dir(directory.path().join("log"))?;
    let mut vc = open(directory.path())?;
    let baseline = vc.create_and_update(11, 12)?;
    vc.branch("topic", None)?;
    vc.switch_branch("topic")?;
    vc.save()?;
    let transaction = vc.begin_transaction()?;
    let created = vc.create_and_update(21, 22)?;
    let expected = vc.transactions().last_logged_sequence();
    // The links-backed journal buffers the marker in memory; make its first
    // flush fail before the owner can know whether storage persisted it.
    std::fs::rename(directory.path().join("log"), directory.path().join("moved"))?;
    let outcome = vc.commit();
    std::fs::rename(directory.path().join("moved"), directory.path().join("log"))?;
    assert!(outcome.is_err(), "the first decision flush must fail");
    let mut persisted_log =
        NamedTypesDecorator::new(directory.path().join("log/log.links"), false)?;
    assert!(
        !persisted_log
            .read_log_entries()?
            .contains(&format!("__transactions:commit:{:032x}", transaction.id)),
        "the marker has not reached this durable file image"
    );
    assert!(vc.branch("blocked", None).is_err());
    assert!(vc.recover().is_err());
    Ok(FailedCheckpoint {
        directory,
        vc,
        baseline,
        created,
        expected,
    })
}

fn verify_retry(cleanup: &str) -> Result<()> {
    let FailedCheckpoint {
        directory,
        mut vc,
        baseline,
        created,
        expected,
    } = uncertain_decision_failure()?;
    std::fs::rename(directory.path().join("log"), directory.path().join("moved"))?;
    let retried = match cleanup {
        "rollback" => vc.rollback(),
        "save" => vc.save(),
        _ => vc.commit(),
    };
    std::fs::rename(directory.path().join("moved"), directory.path().join("log"))?;
    assert!(
        retried.is_err(),
        "a second failed checkpoint retains ownership"
    );
    assert!(vc.branch("still blocked", None).is_err());
    match cleanup {
        "rollback" => vc.rollback()?,
        "save" => vc.save()?,
        _ => vc.commit()?,
    }
    assert_eq!(
        vc.current_sequence(),
        expected,
        "an uncertain commit must resolve and publish through cleanup"
    );
    assert_eq!(vc.transactions().applied_sequence(), expected);
    let entries = vc.transactions_mut().log_store_mut().read_log_entries()?;
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.starts_with("__transactions:commit:"))
            .count(),
        2,
        "uncertain retries must not append duplicate physical decisions"
    );
    assert!(vc.exists(baseline));
    assert!(vc.exists(created));
    vc.tag("committed", None)?;
    vc.save()?;
    drop(vc);
    let mut recovered = open(directory.path())?;
    assert_eq!(recovered.current_sequence(), expected);
    assert_eq!(recovered.try_get_tag("committed"), Some(expected));
    assert!(recovered.exists(baseline));
    assert!(recovered.exists(created));
    recovered.switch_branch("main")?;
    assert!(recovered.exists(baseline));
    assert!(!recovered.exists(created));
    recovered.switch_branch("topic")?;
    assert!(recovered.exists(created));
    recovered.begin_transaction()?;
    let following = recovered.create_and_update(31, 32)?;
    recovered.commit()?;
    recovered.save()?;
    drop(recovered);
    assert!(open(directory.path())?.exists(following));
    Ok(())
}

#[test]
fn rollback_after_uncertain_decision_failure_finishes_publication() -> Result<()> {
    verify_retry("rollback")
}
#[test]
fn save_after_uncertain_decision_failure_finishes_publication() -> Result<()> {
    verify_retry("save")
}
#[test]
fn commit_retry_after_uncertain_decision_failure_finishes_publication() -> Result<()> {
    verify_retry("commit")
}

fn verify_write_fence(operation: &str) -> Result<()> {
    let FailedCheckpoint {
        directory: _directory,
        mut vc,
        baseline,
        ..
    } = uncertain_decision_failure()?;
    let before = vc.transactions().last_logged_sequence();
    let outcome = match operation {
        "create" => vc.create(51, 52).map(|_| ()),
        "composite" => vc.create_and_update(51, 52).map(|_| ()),
        "update" => vc.update(baseline, 51, 52).map(|_| ()),
        "delete" => vc.delete(baseline).map(|_| ()),
        "ensure" => vc.try_ensure_created(20).map(|_| ()),
        "set_name" => NamedTypeLinks::set_name(&mut vc, baseline, "pending").map(|_| ()),
        "remove_name" => NamedTypeLinks::remove_name(&mut vc, baseline),
        _ => vc.get_or_create(51, 52).map(|_| ()),
    };
    assert!(
        outcome.is_err(),
        "{operation} must wait for version publication"
    );
    assert_eq!(vc.transactions().last_logged_sequence(), before);
    vc.commit()?;
    assert!(vc.create_and_update(51, 52).is_ok());
    Ok(())
}
macro_rules! fenced_write {
    ($test:ident, $operation:literal) => {
        #[test]
        fn $test() -> Result<()> {
            verify_write_fence($operation)
        }
    };
}
fenced_write!(create_is_fenced_until_version_publication, "create");
fenced_write!(composite_is_fenced_until_version_publication, "composite");
fenced_write!(update_is_fenced_until_version_publication, "update");
fenced_write!(delete_is_fenced_until_version_publication, "delete");
fenced_write!(ensure_is_fenced_until_version_publication, "ensure");
fenced_write!(
    get_or_create_is_fenced_until_version_publication,
    "get_or_create"
);
fenced_write!(set_name_is_fenced_until_version_publication, "set_name");
fenced_write!(
    remove_name_is_fenced_until_version_publication,
    "remove_name"
);
