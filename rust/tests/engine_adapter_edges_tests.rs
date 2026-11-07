//! Edges of the adapters between the query processor and the engine: reservation
//! under read committed isolation, staged deletes, and the default bounds of the
//! file LSM store.

use link_cli::engine::{Engine, EngineOptions, IsolationLevel, MemoryLog};
use link_cli::{GenericLink, LinksStorage, QueryProcessor};

fn engine(isolation: IsolationLevel) -> Engine<u32, MemoryLog> {
    Engine::open(
        MemoryLog::default(),
        EngineOptions {
            isolation,
            ..EngineOptions::default()
        },
    )
    .unwrap()
}

#[test]
fn a_read_committed_query_reserves_from_the_current_revision() {
    let engine = engine(IsolationLevel::ReadCommitted);
    let mut reader = engine.begin().unwrap();
    // Committed after the reader began, and visible to its reservation.
    let mut writer = engine.begin().unwrap();
    writer.create_link(1, 1).unwrap();
    writer.commit().unwrap();
    QueryProcessor::new(false)
        .process_query(&mut reader, "() ((2 2))")
        .unwrap();
    reader.commit().unwrap();
    let links = engine.snapshot().unwrap().links();
    assert!(links.contains(&GenericLink::new(1, 1, 1)), "{links:?}");
    assert!(links.contains(&GenericLink::new(2, 2, 2)), "{links:?}");
}

#[test]
fn a_query_sees_the_staged_delete_of_its_transaction() {
    let engine = engine(IsolationLevel::Snapshot);
    let mut tx = engine.begin().unwrap();
    let first = tx.create_link(1, 1).unwrap();
    let second = tx.create_link(2, 2).unwrap();
    tx.commit().unwrap();
    let mut tx = engine.begin().unwrap();
    tx.delete_link(first).unwrap();
    QueryProcessor::new(false)
        .process_query(&mut tx, "() ((3 3))")
        .unwrap();
    tx.commit().unwrap();
    let links = engine.snapshot().unwrap().links();
    assert!(!links.iter().any(|link| link.index == first), "{links:?}");
    assert!(links.iter().any(|link| link.index == second), "{links:?}");
}

#[test]
fn a_closed_transaction_reserves_nothing() {
    let engine = engine(IsolationLevel::Snapshot);
    let mut tx = engine.begin().unwrap();
    tx.rollback();
    assert!(QueryProcessor::new(false)
        .process_query(&mut tx, "() ((1 1))")
        .is_err());
}

#[cfg(all(feature = "physical-lsm", feature = "native-pages", any(unix, windows)))]
#[test]
fn file_lsm_options_default_to_finite_bounds_and_no_mirrors() {
    use link_cli::engine::lsm::{FileLsmOptions, LsmStoreOptions};
    let options = FileLsmOptions::default();
    assert_eq!(options.max_pages, 16_384);
    assert!(!options.mirrored_root && !options.mirrored_banks && !options.mirrored_tables);
    assert_eq!(LsmStoreOptions::default().max_namespace_entries, 256);
}

#[cfg(feature = "causal-sessions")]
#[test]
fn causal_named_links_offer_the_panicking_writes() {
    use link_cli::engine::causal_projection::CausalConflictPolicy;
    use link_cli::engine::causal_session::{CausalWorkspace, CausalWorkspaceOptions};
    use link_cli::NamedTypeLinks;
    let mut workspace = CausalWorkspace::new(
        &[],
        1,
        9,
        CausalConflictPolicy::Reject,
        CausalWorkspaceOptions::default(),
    )
    .unwrap();
    let mut named = workspace.named_links();
    let made = named.create(0, 0);
    assert_eq!(named.ensure_created(made), made);
    let pair = named.get_or_create(made, made);
    assert_eq!(named.get_or_create(made, made), pair);
}
