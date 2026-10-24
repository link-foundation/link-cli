use link_cli::engine::{Engine, EngineOptions, MemoryLog};
use link_cli::{GenericLink, LinksStorage, NamedTypeLinks};

#[test]
fn named_sessions_keep_protocol_saves_private_and_report_raw_changes() {
    let engine = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = engine.begin().unwrap();
    assert!(NamedTypeLinks::try_ensure_created(&mut tx, 0).is_err());
    assert_eq!(NamedTypeLinks::ensure_created(&mut tx, 42), 42);
    assert_eq!(NamedTypeLinks::try_ensure_created(&mut tx, 42).unwrap(), 42);
    let id = NamedTypeLinks::create(&mut tx, 42, 0);
    assert_eq!(id, 43);
    assert!(NamedTypeLinks::exists(&mut tx, id));
    assert_eq!(NamedTypeLinks::search(&mut tx, 42, 0), Some(id));
    assert_eq!(NamedTypeLinks::get_or_create(&mut tx, 42, 0), id);
    assert_eq!(NamedTypeLinks::get_or_create(&mut tx, id, 42), 44);
    assert_eq!(
        NamedTypeLinks::get_link(&mut tx, id),
        Some(GenericLink::new(id, 42, 0))
    );
    assert_eq!(NamedTypeLinks::all_links(&mut tx).len(), 3);
    NamedTypeLinks::set_name(&mut tx, id, "雪").unwrap();
    assert_eq!(
        NamedTypeLinks::get_name(&mut tx, id).unwrap().as_deref(),
        Some("雪")
    );
    assert_eq!(
        NamedTypeLinks::get_by_name(&mut tx, "雪").unwrap(),
        Some(id)
    );
    NamedTypeLinks::remove_name(&mut tx, id).unwrap();
    assert!(NamedTypeLinks::get_by_name(&mut tx, "雪")
        .unwrap()
        .is_none());
    assert!(NamedTypeLinks::set_name(&mut tx, 99, "missing").is_err());
    assert!(NamedTypeLinks::remove_name(&mut tx, 99).is_err());
    let mut changes = Vec::new();
    NamedTypeLinks::update_observed(&mut tx, id, id, 42, &mut |before, after| {
        changes.push((before, after))
    })
    .unwrap();
    NamedTypeLinks::delete_observed(&mut tx, id, &mut |before, after| {
        changes.push((before, after))
    })
    .unwrap();
    assert_eq!(
        changes,
        vec![
            (GenericLink::new(id, 42, 0), GenericLink::new(id, id, 42)),
            (GenericLink::new(id, id, 42), GenericLink::null()),
        ]
    );
    // Neither a raw delete nor a duplicate pair silently cascades/merges.
    assert!(NamedTypeLinks::exists(&mut tx, 44));
    assert!(NamedTypeLinks::update(&mut tx, 99, 0, 0).is_err());
    assert!(NamedTypeLinks::delete(&mut tx, 99).is_err());
    NamedTypeLinks::save(&mut tx).unwrap();
    assert!(engine.snapshot().unwrap().links().is_empty());
    LinksStorage::flush(&mut tx).unwrap();
    assert_eq!(engine.snapshot().unwrap().links().len(), 2);
    assert!(NamedTypeLinks::save(&mut tx).is_err());
    assert!(NamedTypeLinks::try_create(&mut tx, 0, 0).is_err());
    assert!(tx.commit().is_err());
}

#[test]
fn allocator_exhaustion_and_missing_updates_leave_no_partial_write() {
    let engine = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = engine.begin().unwrap();
    assert_eq!(tx.ensure_link_created(u32::MAX).unwrap(), u32::MAX);
    assert!(tx.create_link(0, 0).is_err());
    assert!(tx.update_link(99, 0, 0).is_err());
    assert!(tx.delete_link(99).is_err());
    assert_eq!(tx.links_count(), 1);
    tx.rollback();
    tx.rollback();
    assert!(tx.ensure_link_created(1).is_err());
    assert!(tx.set_name(1, Some("closed".into())).is_err());
    assert!(engine.snapshot().unwrap().links().is_empty());
    drop(tx);
    let shared = engine.clone();
    let engine = engine.into_log().err().unwrap();
    drop(shared);
    assert!(engine.into_log().is_ok());
}

#[test]
fn named_allocation_errors_are_returned_to_the_protocol() {
    use link_cli::protocol::{execute_request, format_document, parse_document};
    let engine = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = engine.begin().unwrap();
    tx.ensure_link_created(u32::MAX).unwrap();
    let request = parse_document("(create: (0 0))").unwrap();
    let reply = execute_request(&mut tx, &link_cli::QueryProcessor::new(false), &request);
    assert!(format_document(&reply).contains("error:"));
    assert!(NamedTypeLinks::get_or_create_named(&mut tx, "exhausted").is_err());
    assert_eq!(tx.links_count(), 1);
    assert!(engine.snapshot().unwrap().links().is_empty());
}
