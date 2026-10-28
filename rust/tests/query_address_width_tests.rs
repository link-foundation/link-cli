//! The ordinary query implementation must retain its chosen address width.
use link_cli::engine::{Engine, EngineOptions, MemoryLog};
use link_cli::{GenericLink, GenericQueryProcessor, LinksStorage, NamedTypeLinks};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn ordinary_queries_keep_wide_addresses_through_match_update_and_delete() {
    let id = u64::from(u32::MAX) + 17;
    let engine = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = engine.begin().unwrap();
    let query = GenericQueryProcessor::<u64>::new(false);
    let changes = query
        .process_query(&mut tx, &format!("() (({id}: {id} {id}))"))
        .unwrap();
    assert_eq!(changes, vec![(None, Some(GenericLink::new(id, id, id)))]);
    NamedTypeLinks::set_name(&mut tx, id, "wide").unwrap();
    let changes = query
        .process_query(&mut tx, "((wide: $s $t)) ((wide: $s $t))")
        .unwrap();
    assert_eq!(
        changes,
        vec![(
            Some(GenericLink::new(id, id, id)),
            Some(GenericLink::new(id, id, id))
        )]
    );
    query
        .process_query(&mut tx, &format!("() ((1: {id} {id}))"))
        .unwrap();
    query
        .process_query(&mut tx, &format!("((1: {id} {id})) ((1: 1 {id}))"))
        .unwrap();
    assert_eq!(
        LinksStorage::get_link(&tx, 1),
        Some(GenericLink::new(1, 1, id))
    );
    query
        .process_query(&mut tx, &format!("(({id}: $s $t)) ()"))
        .unwrap();
    assert_eq!(
        LinksStorage::all_links(&tx),
        vec![GenericLink::new(1, 1, id)]
    );
    assert!(engine.snapshot().unwrap().links().is_empty());
    tx.commit().unwrap();
    assert_eq!(
        engine.snapshot().unwrap().links(),
        vec![GenericLink::new(1, 1, id)]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn wide_numeric_names_are_quoted_before_output_is_replayed() {
    let name = (u64::from(u32::MAX) + 17).to_string();
    let engine = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = engine.begin().unwrap();
    let id = tx.create_link(1, 1).unwrap();
    NamedTypeLinks::set_name(&mut tx, id, &name).unwrap();
    let output = NamedTypeLinks::lino_lines(&mut tx).unwrap().join(" ");
    let copy = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut replay = copy.begin().unwrap();
    GenericQueryProcessor::<u64>::new(false)
        .process_query(&mut replay, &format!("() ({output})"))
        .unwrap();
    assert_eq!(replay.get_by_name(&name), Some(1));
    assert_eq!(
        LinksStorage::all_links(&replay),
        vec![GenericLink::new(1, 1, 1)]
    );
}
