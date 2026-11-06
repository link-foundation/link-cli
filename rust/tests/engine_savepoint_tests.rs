//! Savepoints restore staged rows without weakening isolation or reusing addresses.
use link_cli::engine::{
    ConcurrencyControl, Engine, EngineOptions, IsolationLevel, MemoryLog, Savepoint,
    SerializableValidation,
};
use link_cli::{LinksStorage, QueryProcessor};

fn seeded(options: EngineOptions) -> Engine<u32, MemoryLog> {
    let engine = Engine::open(MemoryLog::default(), options).unwrap();
    let mut seed = engine.begin().unwrap();
    seed.create_link(0, 0).unwrap();
    seed.create_link(0, 0).unwrap();
    seed.set_name(1, Some("kept".into())).unwrap();
    seed.commit().unwrap();
    engine
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn rollback_restores_created_updated_deleted_and_renamed_rows() {
    let engine = seeded(EngineOptions::default());
    let mut session = engine.begin().unwrap();
    let staged = session.create_link(1, 1).unwrap();
    let savepoint = session.savepoint().unwrap();
    session.update_link(1, 2, 2).unwrap();
    session.set_name(1, Some("changed".into())).unwrap();
    session.delete_link(2).unwrap();
    session.update_link(staged, 2, 2).unwrap();
    let dropped = session.create_link(0, 0).unwrap();

    session.rollback_to_savepoint(savepoint).unwrap();

    assert_eq!(session.get_link(1).unwrap().source, 0);
    assert_eq!(session.get_name(1).as_deref(), Some("kept"));
    assert_eq!(session.get_by_name("changed"), None);
    assert!(session.get_link(2).is_some());
    assert_eq!(session.get_link(staged).unwrap().source, 1);
    assert!(session.get_link(dropped).is_none());
    assert_eq!(session.all_links().len(), 3);

    // Writes after the rollback stay and the discarded address is not reused.
    let next = session.create_link(0, 0).unwrap();
    assert_eq!(next, dropped + 1);
    session.commit().unwrap();
    let snapshot = engine.snapshot().unwrap();
    assert!(snapshot.get(staged).is_some());
    assert!(snapshot.get(next).is_some());
    assert!(snapshot.get(dropped).is_none());
    assert_eq!(snapshot.get(1).unwrap().link.source, 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn savepoints_nest_stay_usable_and_release_keeps_writes() {
    let engine = seeded(EngineOptions::default());
    let mut session = engine.begin().unwrap();
    let outer = session.savepoint().unwrap();
    session.update_link(1, 1, 1).unwrap();
    let middle = session.savepoint().unwrap();
    session.update_link(1, 2, 2).unwrap();
    let inner = session.savepoint().unwrap();
    session.update_link(1, 1, 2).unwrap();

    // Releasing keeps the writes, and releases later savepoints with it.
    session.release_savepoint(middle).unwrap();
    assert_eq!(session.get_link(1).unwrap().target, 2);
    for gone in [middle, inner] {
        for result in [
            session.rollback_to_savepoint(gone),
            session.release_savepoint(gone),
        ] {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("unknown savepoint"));
        }
    }

    // A savepoint can be rolled back to repeatedly; later ones do not survive it.
    session.rollback_to_savepoint(outer).unwrap();
    assert_eq!(session.get_link(1).unwrap().source, 0);
    let later = session.savepoint().unwrap();
    session.update_link(2, 1, 1).unwrap();
    session.rollback_to_savepoint(outer).unwrap();
    assert_eq!(session.get_link(2).unwrap().source, 0);
    assert!(session.rollback_to_savepoint(later).is_err());
    session.update_link(2, 2, 2).unwrap();
    session.rollback_to_savepoint(outer).unwrap();
    assert_eq!(session.get_link(2).unwrap().source, 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn tokens_of_other_sessions_and_closed_sessions_are_refused() {
    let engine = seeded(EngineOptions::default());
    let mut first = engine.begin().unwrap();
    let mut second = engine.begin().unwrap();
    let foreign = first.savepoint().unwrap();
    second.update_link(1, 1, 1).unwrap();
    assert!(second.rollback_to_savepoint(foreign).is_err());
    assert!(second.release_savepoint(Savepoint(0)).is_err());
    assert_eq!(second.get_link(1).unwrap().source, 1);

    let own = second.savepoint().unwrap();
    second.commit().unwrap();
    for result in [
        second.rollback_to_savepoint(own),
        second.release_savepoint(own),
        second.savepoint().map(|_| ()),
    ] {
        assert!(result.unwrap_err().to_string().contains("closed"));
    }
    first.rollback();
    assert!(first.rollback_to_savepoint(foreign).is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_session_rolled_back_to_no_writes_publishes_no_revision() {
    let engine = seeded(EngineOptions::default());
    let before = engine.snapshot().unwrap().revision;
    let mut session = engine.begin().unwrap();
    let savepoint = session.savepoint().unwrap();
    session.update_link(1, 1, 1).unwrap();
    session.rollback_to_savepoint(savepoint).unwrap();
    assert_eq!(session.commit().unwrap(), before);
    assert_eq!(engine.snapshot().unwrap().revision, before);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn discarded_writes_do_not_conflict_but_retained_writes_do() {
    let engine = seeded(EngineOptions {
        isolation: IsolationLevel::Snapshot,
        ..EngineOptions::default()
    });
    let mut session = engine.begin().unwrap();
    session.update_link(2, 1, 1).unwrap();
    let savepoint = session.savepoint().unwrap();
    session.update_link(1, 1, 1).unwrap();
    session.rollback_to_savepoint(savepoint).unwrap();

    let mut other = engine.begin().unwrap();
    other.update_link(1, 2, 2).unwrap();
    other.commit().unwrap();
    session.commit().unwrap();
    assert_eq!(engine.snapshot().unwrap().get(1).unwrap().link.source, 2);

    let mut session = engine.begin().unwrap();
    session.update_link(2, 2, 2).unwrap();
    let savepoint = session.savepoint().unwrap();
    session.rollback_to_savepoint(savepoint).unwrap();
    let mut other = engine.begin().unwrap();
    other.update_link(2, 0, 0).unwrap();
    other.commit().unwrap();
    assert!(session
        .commit()
        .unwrap_err()
        .to_string()
        .contains("conflict"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn reads_before_a_rollback_stay_validated() {
    let engine = seeded(EngineOptions {
        isolation: IsolationLevel::Serializable,
        serializable_validation: SerializableValidation::ReadSet,
        ..EngineOptions::default()
    });
    let mut session = engine.begin().unwrap();
    let savepoint = session.savepoint().unwrap();
    let observed = session.get_link(1).unwrap();
    session
        .update_link(2, observed.source, observed.target)
        .unwrap();
    session.rollback_to_savepoint(savepoint).unwrap();
    session.update_link(2, 1, 1).unwrap();

    let mut other = engine.begin().unwrap();
    other.update_link(1, 1, 1).unwrap();
    other.commit().unwrap();
    let failure = session.commit().unwrap_err().to_string();
    assert!(failure.contains("a read link changed"), "{failure}");
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn point_write_ownership_lasts_until_the_session_ends() {
    let engine = seeded(EngineOptions {
        concurrency: ConcurrencyControl::Pessimistic,
        ..EngineOptions::default()
    });
    let mut session = engine.begin().unwrap();
    let savepoint = session.savepoint().unwrap();
    session.update_link(1, 1, 1).unwrap();
    session.rollback_to_savepoint(savepoint).unwrap();

    let mut other = engine.begin().unwrap();
    let busy = other.update_link(1, 2, 2).unwrap_err().to_string();
    assert!(busy.contains("busy"), "{busy}");
    session.rollback();
    other.update_link(1, 2, 2).unwrap();
    other.commit().unwrap();
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn read_committed_sees_commits_made_after_the_savepoint() {
    let engine = seeded(EngineOptions {
        isolation: IsolationLevel::ReadCommitted,
        ..EngineOptions::default()
    });
    let mut session = engine.begin().unwrap();
    let savepoint = session.savepoint().unwrap();
    session.update_link(1, 1, 1).unwrap();
    let mut other = engine.begin().unwrap();
    other.update_link(1, 2, 2).unwrap();
    other.commit().unwrap();
    assert_eq!(session.get_link(1).unwrap().source, 1);
    session.rollback_to_savepoint(savepoint).unwrap();
    assert_eq!(session.get_link(1).unwrap().source, 2);
}

#[test]
fn query_addresses_stay_consumed_after_rollback() {
    let engine = seeded(EngineOptions::default());
    let mut session = engine.begin().unwrap();
    let savepoint = session.savepoint().unwrap();
    QueryProcessor::new(false)
        .process_query(&mut session, "(() ((1 1)))")
        .unwrap();
    assert_eq!(session.links_count(), 3);
    session.rollback_to_savepoint(savepoint).unwrap();
    assert_eq!(session.links_count(), 2);
    assert_eq!(session.create_link(0, 0).unwrap(), 4);
}
