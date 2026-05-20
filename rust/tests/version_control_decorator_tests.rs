//! Integration tests for the optional version-control decorator.
//!
//! Mirrors the C# `VersionControlDecoratorTests` and exercises R11–R16
//! of issue #94.

use anyhow::Result;
use link_cli::{
    CommitMode, LogRetentionPolicy, NamedTypesDecorator, TransactionsDecorator,
    VersionControlDecorator, DEFAULT_BRANCH_NAME,
};
use tempfile::NamedTempFile;

struct VcGuard {
    _data_file: NamedTempFile,
    _log_file: NamedTempFile,
    _vc_file: NamedTempFile,
}

fn make_vc() -> (VersionControlDecorator, VcGuard) {
    let data_file = NamedTempFile::new().unwrap();
    let log_file = NamedTempFile::new().unwrap();
    let vc_file = NamedTempFile::new().unwrap();
    let data_links = NamedTypesDecorator::new(data_file.path(), false).unwrap();
    let log_links = NamedTypesDecorator::new(log_file.path(), false).unwrap();
    let vc_links = NamedTypesDecorator::new(vc_file.path(), false).unwrap();
    let tx = TransactionsDecorator::new(
        data_links,
        log_links,
        LogRetentionPolicy::default(),
        CommitMode::default(),
        false,
    )
    .unwrap();
    let vc = VersionControlDecorator::new(tx, vc_links, false).unwrap();
    (
        vc,
        VcGuard {
            _data_file: data_file,
            _log_file: log_file,
            _vc_file: vc_file,
        },
    )
}

#[test]
fn default_branch_exists_on_first_open() {
    let (vc, _guard) = make_vc();
    assert_eq!(DEFAULT_BRANCH_NAME, vc.current_branch());
    let branches = vc.list_branches();
    assert_eq!(1, branches.len());
    assert_eq!(DEFAULT_BRANCH_NAME, branches[0].name);
}

#[test]
fn new_transitions_are_attributed_to_current_branch() -> Result<()> {
    let (mut vc, _guard) = make_vc();
    let _id = vc.create_and_update(0, 0)?;
    let head = vc.transactions().last_logged_sequence();
    assert!(
        head >= 2,
        "create_and_update must produce at least two transitions (got {head})."
    );
    assert_eq!(head, vc.current_sequence());
    Ok(())
}

#[test]
fn checkout_to_zero_rewinds_everything() -> Result<()> {
    let (mut vc, _guard) = make_vc();
    let a = vc.create_and_update(0, 0)?;
    let b = vc.create_and_update(0, 0)?;
    assert!(vc.exists(a));
    assert!(vc.exists(b));

    vc.checkout(0)?;

    assert!(!vc.exists(a), "all links must be rewound after checkout 0");
    assert!(!vc.exists(b));
    assert_eq!(0, vc.current_sequence());
    Ok(())
}

#[test]
fn checkout_and_forward_replay_restores_state() -> Result<()> {
    let (mut vc, _guard) = make_vc();
    let a = vc.create_and_update(0, 0)?;
    let after_first = vc.transactions().last_logged_sequence();
    let b = vc.create_and_update(0, 0)?;
    let after_second = vc.transactions().last_logged_sequence();

    vc.checkout(after_first)?;
    assert!(vc.exists(a), "first link must remain after partial rewind");
    assert!(!vc.exists(b), "second link must disappear after partial rewind");

    vc.checkout(after_second)?;
    assert!(vc.exists(a));
    assert!(vc.exists(b), "second link must reappear after forward checkout");
    Ok(())
}

#[test]
fn branch_forks_from_current_head() -> Result<()> {
    let (mut vc, _guard) = make_vc();
    vc.create_and_update(0, 0)?;
    vc.branch("feature", None)?;
    assert!(vc.list_branches().iter().any(|b| b.name == "feature"));
    Ok(())
}

#[test]
fn switch_branch_applies_and_rewinds_transitions() -> Result<()> {
    let (mut vc, _guard) = make_vc();
    let a = vc.create_and_update(0, 0)?;
    let head_before_branch = vc.current_sequence();

    vc.branch("feature", None)?;
    vc.switch_branch("feature")?;
    assert_eq!("feature", vc.current_branch());

    let b = vc.create_and_update(0, 0)?;
    assert!(vc.exists(b));
    let feature_head = vc.current_sequence();

    vc.switch_branch(DEFAULT_BRANCH_NAME)?;
    assert_eq!(DEFAULT_BRANCH_NAME, vc.current_branch());
    assert!(vc.exists(a), "main-branch link must remain after switching back");
    assert!(!vc.exists(b), "feature-branch link must disappear after switching back to main");
    assert_eq!(head_before_branch, vc.current_sequence());

    vc.switch_branch("feature")?;
    assert!(vc.exists(a));
    assert!(vc.exists(b), "feature-branch link must reappear");
    assert_eq!(feature_head, vc.current_sequence());
    Ok(())
}

#[test]
fn tag_points_to_current_head() -> Result<()> {
    let (mut vc, _guard) = make_vc();
    vc.create_and_update(0, 0)?;
    vc.tag("v1", None)?;
    let seq = vc.try_get_tag("v1").expect("tag must be retrievable");
    assert_eq!(vc.current_sequence(), seq);
    assert!(vc.list_tags().contains_key("v1"));
    Ok(())
}

#[test]
fn branch_from_explicit_seq_uses_given_point() -> Result<()> {
    let (mut vc, _guard) = make_vc();
    vc.create_and_update(0, 0)?;
    let first_head = vc.current_sequence();
    vc.create_and_update(0, 0)?;

    vc.branch("backport", Some(first_head))?;
    let branches = vc.list_branches();
    let branch = branches
        .iter()
        .find(|b| b.name == "backport")
        .expect("backport branch must exist");
    assert_eq!(first_head, branch.fork_seq);
    Ok(())
}

#[test]
fn recover_rebuilds_state_from_branches_store() -> Result<()> {
    let data_file = NamedTempFile::new()?;
    let log_file = NamedTempFile::new()?;
    let vc_file = NamedTempFile::new()?;
    let data_path = data_file.path().to_path_buf();
    let log_path = log_file.path().to_path_buf();
    let vc_path = vc_file.path().to_path_buf();

    {
        let data_links = NamedTypesDecorator::new(&data_path, false)?;
        let log_links = NamedTypesDecorator::new(&log_path, false)?;
        let vc_links = NamedTypesDecorator::new(&vc_path, false)?;
        let tx = TransactionsDecorator::new(
            data_links,
            log_links,
            LogRetentionPolicy::default(),
            CommitMode::default(),
            false,
        )?;
        let mut vc = VersionControlDecorator::new(tx, vc_links, false)?;
        vc.create_and_update(0, 0)?;
        vc.tag("checkpoint", None)?;
        vc.branch("feature", None)?;
        vc.save()?;
    }

    let data_links = NamedTypesDecorator::new(&data_path, false)?;
    let log_links = NamedTypesDecorator::new(&log_path, false)?;
    let vc_links = NamedTypesDecorator::new(&vc_path, false)?;
    let tx = TransactionsDecorator::new(
        data_links,
        log_links,
        LogRetentionPolicy::default(),
        CommitMode::default(),
        false,
    )?;
    let reopened = VersionControlDecorator::new(tx, vc_links, false)?;
    assert!(reopened
        .list_branches()
        .iter()
        .any(|b| b.name == "feature"));
    assert!(reopened.try_get_tag("checkpoint").is_some());
    Ok(())
}

#[test]
fn checkout_out_of_range_throws() -> Result<()> {
    let (mut vc, _guard) = make_vc();
    vc.create_and_update(0, 0)?;
    assert!(vc.checkout(999).is_err());
    Ok(())
}

#[test]
fn duplicate_branch_throws() -> Result<()> {
    let (mut vc, _guard) = make_vc();
    vc.branch("feature", None)?;
    assert!(vc.branch("feature", None).is_err());
    Ok(())
}
