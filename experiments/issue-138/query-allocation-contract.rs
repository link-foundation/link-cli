//! Install as rust/tests/engine_query_allocation_tests.rs for the pre-fix run.
use link_cli::engine::{Engine, EngineOptions, MemoryLog};
use link_cli::{GenericLink, LinksStorage, QueryProcessor};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn anonymous_forward_reference_does_not_consume_its_predicted_identity() {
    let engine = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = engine.begin().unwrap();
    QueryProcessor::new(false).process_query(&mut tx, "(() ((1 1)))").unwrap();
    assert_eq!(LinksStorage::all_links(&tx), vec![GenericLink::new(1, 1, 1)]);
    assert!(engine.snapshot().unwrap().links().is_empty());
    tx.commit().unwrap();
    assert_eq!(engine.snapshot().unwrap().links(), vec![GenericLink::new(1, 1, 1)]);
}
