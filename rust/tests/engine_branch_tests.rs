use link_cli::engine::{Engine, EngineOptions, HistoryPolicy, MemoryLog};
use link_cli::LinksStorage;

fn database() -> Engine<u32, MemoryLog> {
    Engine::open(MemoryLog::default(), EngineOptions::default()).unwrap()
}

#[test]
fn branches_merge_independent_changes_and_reject_overlapping_edits() {
    let parent = database();
    let mut seed = parent.begin().unwrap();
    seed.ensure_link_created(1).unwrap();
    seed.ensure_link_created(2).unwrap();
    seed.commit().unwrap();
    let mut branch = parent
        .fork(MemoryLog::default(), EngineOptions::default())
        .unwrap();
    let mut local = parent.begin().unwrap();
    local.update_link(1, 1, 0).unwrap();
    local.commit().unwrap();
    let mut remote = branch.engine().begin().unwrap();
    remote.set_name(2, Some("branch".into())).unwrap();
    remote.commit().unwrap();
    assert!(parent.snapshot().unwrap().get(2).unwrap().name.is_none());
    assert_eq!(branch.merge().unwrap(), 3);
    assert_eq!(parent.snapshot().unwrap().get(1).unwrap().link.source, 1);
    assert_eq!(
        parent.snapshot().unwrap().get(2).unwrap().name.as_deref(),
        Some("branch")
    );
    assert_eq!(branch.merge().unwrap(), 3, "a repeated merge is a no-op");
    let mut remote = branch.engine().begin().unwrap();
    remote.set_name(2, Some("again".into())).unwrap();
    remote.commit().unwrap();
    assert_eq!(
        branch.merge().unwrap(),
        4,
        "incremental merges advance their base"
    );
    let mut local = parent.begin().unwrap();
    local.delete_link(2).unwrap();
    local.commit().unwrap();
    let mut remote = branch.engine().begin().unwrap();
    remote.set_name(2, Some("conflicting".into())).unwrap();
    remote.commit().unwrap();
    assert!(branch.merge().unwrap_err().to_string().contains("conflict"));
    assert!(parent.snapshot().unwrap().get(2).is_none());
}

#[test]
fn branch_merge_base_survives_reopening_and_history_eviction() {
    let parent = database();
    let mut seed = parent.begin().unwrap();
    seed.create_link(0, 0).unwrap();
    seed.commit().unwrap();
    let branch = parent
        .fork(
            MemoryLog::default(),
            EngineOptions {
                history: HistoryPolicy::Last(1),
                ..Default::default()
            },
        )
        .unwrap();
    let base = branch.merge_base_record();
    let mut remote = branch.engine().begin().unwrap();
    remote.delete_link(1).unwrap();
    remote.commit().unwrap();
    drop(remote);
    let log = branch.into_engine().into_log().ok().unwrap();
    let mut resumed = parent
        .resume_branch(log, EngineOptions::default(), &base)
        .unwrap();
    assert_eq!(resumed.merge().unwrap(), 2);
    assert!(parent.snapshot().unwrap().links().is_empty());
    let mut tx = parent.begin().unwrap();
    assert_eq!(
        tx.create_link(0, 0).unwrap(),
        2,
        "merge must preserve the allocator"
    );
}

#[test]
fn fork_refuses_existing_logs_and_resume_requires_a_real_branch_revision() {
    let parent = database();
    let branch = parent
        .fork(MemoryLog::default(), EngineOptions::default())
        .unwrap();
    let base = branch.merge_base_record();
    let log = branch.into_engine().into_log().ok().unwrap();
    assert!(parent.fork(log.clone(), EngineOptions::default()).is_err());
    assert!(parent
        .resume_branch(MemoryLog::default(), EngineOptions::default(), &base)
        .is_err());
    assert!(parent
        .resume_branch(log, EngineOptions::default(), "invalid")
        .is_err());
}

#[test]
fn merge_obeys_exclusive_admission_constraints_and_conflict_validation() {
    use link_cli::engine::{ConcurrencyControl, Constraint, IsolationLevel};
    let parent = Engine::<u32, _>::open(
        MemoryLog::default(),
        EngineOptions {
            concurrency: ConcurrencyControl::Exclusive,
            constraints: vec![Constraint::References],
            ..Default::default()
        },
    )
    .unwrap();
    let mut seed = parent.begin().unwrap();
    seed.create_link(0, 0).unwrap();
    seed.commit().unwrap();
    let mut branch = parent
        .fork(MemoryLog::default(), EngineOptions::default())
        .unwrap();
    let mut remote = branch.engine().begin().unwrap();
    remote.update_link(1, 999, 0).unwrap();
    remote.commit().unwrap();
    let held = parent.begin().unwrap();
    assert!(branch.merge().unwrap_err().to_string().contains("busy"));
    drop(held);
    assert!(branch
        .merge()
        .unwrap_err()
        .to_string()
        .contains("constraint"));
    assert_eq!(parent.snapshot().unwrap().get(1).unwrap().link.source, 0);
    let mut remote = branch.engine().begin().unwrap();
    remote.update_link(1, 1, 0).unwrap();
    remote.commit().unwrap();
    branch.merge().unwrap();

    let parent = Engine::<u32, _>::open(
        MemoryLog::default(),
        EngineOptions {
            isolation: IsolationLevel::Snapshot,
            ..Default::default()
        },
    )
    .unwrap();
    let mut branch = parent
        .fork(MemoryLog::default(), EngineOptions::default())
        .unwrap();
    let mut old = parent.begin().unwrap();
    old.ensure_link_created(1).unwrap();
    let mut remote = branch.engine().begin().unwrap();
    remote.create_link(1, 1).unwrap();
    remote.commit().unwrap();
    branch.merge().unwrap();
    assert!(
        old.commit().is_err(),
        "merge must update written-row conflict generations"
    );
}

#[test]
fn allocator_only_merge_is_durable_even_when_no_rows_survive() {
    let parent = database();
    let mut branch = parent
        .fork(MemoryLog::default(), EngineOptions::default())
        .unwrap();
    let mut remote = branch.engine().begin().unwrap();
    let id = remote.create_link(0, 0).unwrap();
    remote.delete_link(id).unwrap();
    remote.commit().unwrap();
    drop(remote);
    assert_eq!(branch.merge().unwrap(), 1);
    assert!(parent.snapshot().unwrap().links().is_empty());
    drop(branch);
    let log = parent.into_log().ok().unwrap();
    let reopened = Engine::<u32, _>::open(log, EngineOptions::default()).unwrap();
    assert_eq!(reopened.begin().unwrap().create_link(0, 0).unwrap(), 2);
}
