//! Allocation planning must follow execution without staging probe rows.
use link_cli::engine::{Engine, EngineOptions, IsolationLevel, MemoryLog};
use link_cli::{GenericLink, LinksStorage, QueryProcessor};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn anonymous_forward_reference_does_not_consume_its_predicted_identity() {
    let engine = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = engine.begin().unwrap();
    QueryProcessor::new(false)
        .process_query(&mut tx, "(() ((1 1)))")
        .unwrap();
    assert_eq!(
        LinksStorage::all_links(&tx),
        vec![GenericLink::new(1, 1, 1)]
    );
    assert!(engine.snapshot().unwrap().links().is_empty());
    tx.commit().unwrap();
    assert_eq!(
        engine.snapshot().unwrap().links(),
        vec![GenericLink::new(1, 1, 1)]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn allocations_follow_explicit_ensures_names_and_pair_reuse() {
    for (query, expected) in [
        ("() ((1 2) (2 2))", vec![(1, 1, 2), (2, 2, 2)]),
        ("() ((1: 1 1) (2 2))", vec![(1, 1, 1), (2, 2, 2)]),
        ("() ((foo: (1 1) (1 1)))", vec![(1, 1, 1)]),
        ("() ((foo: foo foo) (2 2))", vec![(1, 1, 1), (2, 2, 2)]),
        (
            "() ((1 1) (2: 2 2) (3 3))",
            vec![(1, 1, 1), (2, 2, 2), (3, 3, 3)],
        ),
        (
            "() ((100: 100 100) (101 101))",
            vec![(100, 100, 100), (101, 101, 101)],
        ),
    ] {
        let engine =
            Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
        let mut tx = engine.begin().unwrap();
        QueryProcessor::new(false)
            .process_query(&mut tx, query)
            .unwrap();
        let expected = expected
            .into_iter()
            .map(|(id, s, t)| GenericLink::new(id, s, t))
            .collect::<Vec<_>>();
        assert_eq!(LinksStorage::all_links(&tx), expected, "{query}");
        assert!(engine.snapshot().unwrap().links().is_empty());
        tx.commit().unwrap();
        assert_eq!(engine.snapshot().unwrap().links(), expected, "{query}");
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn failed_validation_keeps_owned_ids_separate_from_other_sessions() {
    let engine = Engine::<u32, _>::open(
        MemoryLog::default(),
        EngineOptions {
            isolation: IsolationLevel::Snapshot,
            ..Default::default()
        },
    )
    .unwrap();
    let mut first = engine.begin().unwrap();
    let processor = QueryProcessor::new(false);
    assert!(processor.process_query(&mut first, "() ((9 9))").is_err());
    assert!(LinksStorage::all_links(&first).is_empty());
    let mut second = engine.begin().unwrap();
    processor.process_query(&mut second, "() ((2 2))").unwrap();
    second.commit().unwrap();
    processor.process_query(&mut first, "() ((1 1))").unwrap();
    first.commit().unwrap();
    assert_eq!(
        engine.snapshot().unwrap().links(),
        vec![GenericLink::new(1, 1, 1), GenericLink::new(2, 2, 2)]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn explicit_ensure_retires_an_owned_id_and_auto_creation_keeps_the_prefix() {
    let engine = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = engine.begin().unwrap();
    assert!(QueryProcessor::new(false)
        .process_query(&mut tx, "() ((9 9))")
        .is_err());
    tx.ensure_link_created(1).unwrap();
    tx.update_link(1, 1, 1).unwrap();
    assert_eq!(tx.create_link(0, 0).unwrap(), 2);
    assert_eq!(tx.get_link(1), Some(GenericLink::new(1, 1, 1)));
    tx.rollback();
    assert!(engine.snapshot().unwrap().links().is_empty());

    let engine = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = engine.begin().unwrap();
    QueryProcessor::new(false)
        .with_auto_create_missing_references(true)
        .process_query(&mut tx, "() ((100 1))")
        .unwrap();
    assert_eq!(
        LinksStorage::all_links(&tx),
        vec![GenericLink::new(1, 100, 1), GenericLink::new(100, 100, 100)]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn existing_pairs_do_not_reserve_extra_addresses_and_syntax_failure_does_not_allocate() {
    let engine = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = engine.begin().unwrap();
    let processor = QueryProcessor::new(false);
    assert!(processor.process_query(&mut tx, "(() ((").is_err());
    assert_eq!(tx.create_link(1, 1).unwrap(), 1);
    processor
        .process_query(&mut tx, "() ((1 1) (2 2))")
        .unwrap();
    assert_eq!(tx.create_link(0, 0).unwrap(), 3);
    assert_eq!(
        LinksStorage::all_links(&tx),
        vec![
            GenericLink::new(1, 1, 1),
            GenericLink::new(2, 2, 2),
            GenericLink::new(3, 0, 0)
        ]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn zero_references_remain_bound_during_swaps_equality_and_joins() {
    for (query, seed, expected) in [
        (
            "((1: $s $t)) ((1: $t $s))",
            vec![(1, 1, 0)],
            vec![(1, 0, 1)],
        ),
        (
            "(($i: $x $x)) ()",
            vec![(1, 0, 1), (2, 2, 2)],
            vec![(1, 0, 1)],
        ),
        (
            "((1: $x *) (2: $x *)) ((1: $x 1))",
            vec![(1, 0, 1), (2, 1, 1)],
            vec![(1, 0, 1), (2, 1, 1)],
        ),
    ] {
        let engine =
            Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
        let mut tx = engine.begin().unwrap();
        for (id, source, target) in seed {
            tx.ensure_link_created(id).unwrap();
            tx.update_link(id, source, target).unwrap();
        }
        QueryProcessor::new(false)
            .process_query(&mut tx, query)
            .unwrap();
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(id, s, t)| GenericLink::new(id, s, t))
            .collect();
        assert_eq!(LinksStorage::all_links(&tx), expected, "{query}");
        assert!(engine.snapshot().unwrap().links().is_empty());
        tx.commit().unwrap();
        assert_eq!(engine.snapshot().unwrap().links(), expected, "{query}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn concurrent_query_owners_reserve_disjoint_addresses() {
    let engine = Engine::<u32, _>::open(
        MemoryLog::default(),
        EngineOptions {
            isolation: IsolationLevel::Snapshot,
            ..Default::default()
        },
    )
    .unwrap();
    let start = std::sync::Arc::new(std::sync::Barrier::new(8));
    let handles = (0..8)
        .map(|_| {
            let engine = engine.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                let mut tx = engine.begin().unwrap();
                start.wait();
                let changes = QueryProcessor::new(false)
                    .process_query(&mut tx, "() ((* *))")
                    .unwrap();
                let id = changes[0].1.unwrap().index;
                tx.commit().unwrap();
                id
            })
        })
        .collect::<Vec<_>>();
    let mut ids = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    assert_eq!(ids, (1..=8).collect::<Vec<_>>());
    assert_eq!(engine.snapshot().unwrap().links().len(), 8);
}
