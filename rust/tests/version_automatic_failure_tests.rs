//! Automatic versioned writes retain their implicit publication owner on error.
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
struct Failure {
    directory: tempfile::TempDir,
    vc: VersionControlDecorator,
    id: u32,
    original: i64,
    expected: i64,
}
fn failure(phase: &str) -> Result<Failure> {
    let directory = tempfile::tempdir()?;
    std::fs::create_dir(directory.path().join("data"))?;
    std::fs::create_dir(directory.path().join("log"))?;
    let mut vc = open(directory.path())?;
    let id = vc.create_and_update(11, 12)?;
    NamedTypeLinks::set_name(&mut vc, id, "original")?;
    let original = vc.current_sequence();
    vc.branch("topic", None)?;
    vc.switch_branch("topic")?;
    vc.save()?;
    std::fs::rename(directory.path().join(phase), directory.path().join("moved"))?;
    let outcome = vc.update(id, 21, 22);
    std::fs::rename(directory.path().join("moved"), directory.path().join(phase))?;
    assert!(outcome.is_err(), "automatic {phase} persistence must fail");
    let expected = vc.transactions().last_logged_sequence();
    assert!(expected > original);
    assert_eq!(vc.get(id).unwrap().source, 21);
    let mut persisted_log =
        NamedTypesDecorator::new(directory.path().join("log/log.links"), false)?;
    assert_eq!(
        persisted_log
            .read_log_entries()?
            .iter()
            .filter(|entry| entry.starts_with("__transactions:commit:"))
            .count(),
        if phase == "data" { 2 } else { 1 }
    );
    assert!(vc.branch("blocked", None).is_err());
    assert!(vc.tag("blocked", None).is_err());
    assert!(vc.checkout(0).is_err());
    assert!(vc.switch_branch("main").is_err());
    assert!(vc.recover().is_err());
    assert!(vc.begin_transaction().is_err());
    Ok(Failure {
        directory,
        vc,
        id,
        original,
        expected,
    })
}
fn cleanup(vc: &mut VersionControlDecorator, operation: &str) -> Result<()> {
    match operation {
        "rollback" => vc.rollback(),
        "save" => vc.save(),
        _ => vc.commit(),
    }
}
fn verify_retry(operation: &str) -> Result<()> {
    for phase in ["data", "log"] {
        let Failure {
            directory,
            mut vc,
            id,
            original,
            expected,
        } = failure(phase)?;
        std::fs::rename(directory.path().join(phase), directory.path().join("moved"))?;
        let repeated = cleanup(&mut vc, operation);
        std::fs::rename(directory.path().join("moved"), directory.path().join(phase))?;
        assert!(repeated.is_err());
        assert_eq!(vc.get(id).unwrap().source, 21);
        assert!(vc.branch("still blocked", None).is_err());
        cleanup(&mut vc, operation)?;
        assert_eq!(
            vc.current_sequence(),
            expected,
            "automatic publication lost through {operation} at {phase}"
        );
        assert_eq!(vc.transactions().applied_sequence(), expected);
        let entries = vc.transactions_mut().log_store_mut().read_log_entries()?;
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.starts_with("__transactions:commit:"))
                .count(),
            2
        );
        vc.tag("committed", None)?;
        vc.save()?;
        drop(vc);
        let mut vc = open(directory.path())?;
        assert_eq!(vc.current_sequence(), expected);
        assert_eq!(vc.try_get_tag("committed"), Some(expected));
        assert_eq!(vc.get(id).unwrap().source, 21);
        assert_eq!(
            NamedTypeLinks::get_name(&mut vc, id)?.as_deref(),
            Some("original")
        );
        vc.checkout(original)?;
        assert_eq!(vc.get(id).unwrap().source, 11);
        vc.checkout(expected)?;
        assert_eq!(vc.get(id).unwrap().source, 21);
        vc.update(id, 31, 32)?;
        vc.save()?;
        drop(vc);
        let vc = open(directory.path())?;
        assert_eq!(vc.current_sequence(), expected + 1);
        assert_eq!(vc.get(id).unwrap().source, 31);
    }
    Ok(())
}
fn verify_write(operation: &str) -> Result<()> {
    for phase in ["data", "log"] {
        let Failure {
            directory: _directory,
            mut vc,
            id,
            expected,
            ..
        } = failure(phase)?;
        let outcome = match operation {
            "create" => vc.create(51, 52).map(|_| ()),
            "composite" => vc.create_and_update(51, 52).map(|_| ()),
            "update" => vc.update(id, 51, 52).map(|_| ()),
            "delete" => vc.delete(id).map(|_| ()),
            "ensure" => vc.try_ensure_created(20).map(|_| ()),
            "set" => NamedTypeLinks::set_name(&mut vc, id, "changed").map(|_| ()),
            "remove" => NamedTypeLinks::remove_name(&mut vc, id),
            _ => vc.get_or_create(51, 52).map(|_| ()),
        };
        assert!(
            outcome.is_err(),
            "{operation} bypasses pending automatic publication"
        );
        assert_eq!(vc.transactions().last_logged_sequence(), expected);
        assert_eq!(vc.get(id).unwrap().source, 21);
        assert_eq!(
            NamedTypeLinks::get_name(&mut vc, id)?.as_deref(),
            Some("original")
        );
        vc.commit()?;
        assert_eq!(vc.current_sequence(), expected);
        vc.create_and_update(51, 52)?;
    }
    Ok(())
}
macro_rules! contract {
    ($name:ident, $helper:ident, $operation:literal) => {
        #[test]
        fn $name() -> Result<()> {
            $helper($operation)
        }
    };
}
contract!(
    commit_finishes_automatic_publication,
    verify_retry,
    "commit"
);
contract!(
    rollback_finishes_automatic_publication,
    verify_retry,
    "rollback"
);
contract!(save_finishes_automatic_publication, verify_retry, "save");
contract!(
    create_waits_for_automatic_publication,
    verify_write,
    "create"
);
contract!(
    composite_waits_for_automatic_publication,
    verify_write,
    "composite"
);
contract!(
    update_waits_for_automatic_publication,
    verify_write,
    "update"
);
contract!(
    delete_waits_for_automatic_publication,
    verify_write,
    "delete"
);
contract!(
    ensure_waits_for_automatic_publication,
    verify_write,
    "ensure"
);
contract!(
    get_or_create_waits_for_automatic_publication,
    verify_write,
    "get"
);
contract!(
    set_name_waits_for_automatic_publication,
    verify_write,
    "set"
);
contract!(
    remove_name_waits_for_automatic_publication,
    verify_write,
    "remove"
);
