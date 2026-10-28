//! Heap-doublets contracts shared by native and actual WASM runtimes.
//! File durability, mapping and advisory locks retain their native tests.
use link_cli::doublets::data::LinkReference;
use link_cli::doublets::decorators::DecoratorsExt;
use link_cli::doublets::mem::Global;
use link_cli::doublets::unit::{LinkPart, Store};
use link_cli::doublets::Doublets;
use link_cli::{
    CommitMode, DoubletsStorage, GenericLink, GenericTransactionsDecorator, LinkError, LinkStorage,
    LinksStorage, LogRetentionPolicy, NamedTypesDecorator,
};

type Heap<T> = DoubletsStorage<T, Store<T, Global<LinkPart<T>>>>;

fn heap<T: LinkReference>() -> Heap<T> {
    DoubletsStorage::wrap(Store::<T, _>::new(Global::new()).unwrap())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn owned_heap_crud_returns_previous_values_and_reuses_deleted_addresses() {
    let mut storage = heap::<u32>();
    assert_eq!(storage.links_count(), 0);
    assert_eq!(storage.get_link(1), None);
    assert!(!storage.link_exists(1));
    assert_eq!(storage.create_link(0, 0).unwrap(), 1);
    assert_eq!(storage.create_link(1, 1).unwrap(), 2);
    assert_eq!(storage.get_or_create_link(1, 1).unwrap(), 2);
    assert_eq!(storage.get_or_create_link(1, 2).unwrap(), 3);
    assert_eq!(storage.search_link(1, 2), Some(3));
    assert_eq!(storage.search_link(2, 1), None);
    let previous = GenericLink::new(3, 1, 2);
    assert_eq!(storage.update_link(3, 2, 1).unwrap(), previous);
    let updated = GenericLink::new(3, 2, 1);
    assert_eq!(storage.get_link(3), Some(updated));
    assert_eq!(storage.delete_link(3).unwrap(), updated);
    assert_eq!(storage.create_link(2, 2).unwrap(), 3);
    assert_eq!(
        storage.all_links(),
        vec![
            GenericLink::new(1, 0, 0),
            GenericLink::new(2, 1, 1),
            GenericLink::new(3, 2, 2),
        ]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn patterns_intersect_index_source_and_target_without_mutating_rows() {
    let mut storage = heap::<u32>();
    for (source, target) in [(0, 0), (1, 1), (1, 2), (2, 1), (1, 1)] {
        storage.create_link(source, target).unwrap();
    }
    let baseline = storage.all_links();
    // Every combination of optional index/source/target has an independent
    // filter oracle, including absent addresses and zero-valued endpoints.
    for index in [None, Some(0), Some(2), Some(9), Some(u32::MAX)] {
        for source in [None, Some(0), Some(1), Some(9), Some(u32::MAX)] {
            for target in [None, Some(0), Some(1), Some(9), Some(u32::MAX)] {
                let expected: Vec<_> = baseline
                    .iter()
                    .copied()
                    .filter(|row| {
                        index.is_none_or(|id| row.index == id)
                            && source.is_none_or(|s| row.source == s)
                            && target.is_none_or(|t| row.target == t)
                    })
                    .collect();
                let mut actual = storage.query_links(index, source, target);
                actual.sort_unstable_by_key(|row| row.index);
                assert_eq!(
                    actual, expected,
                    "index={index:?}, source={source:?}, target={target:?}"
                );
            }
        }
    }
    assert_eq!(storage.all_links(), baseline);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn null_endpoints_are_searchable_and_get_or_create_reuses_the_existing_row() {
    for (source, target) in [
        (0, 0),
        (0, 1),
        (1, 0),
        (u32::MAX, 1),
        (1, u32::MAX),
        (u32::MAX, u32::MAX),
    ] {
        let mut storage = heap::<u32>();
        storage.create_link(1, 1).unwrap();
        assert_eq!(storage.search_link(source, target), None);
        let id = storage.create_link(source, target).unwrap();
        let baseline = storage.all_links();
        assert_eq!(
            storage.search_link(source, target),
            Some(id),
            "{source}, {target}"
        );
        assert_eq!(storage.get_or_create_link(source, target).unwrap(), id);
        assert_eq!(storage.all_links(), baseline);
        assert_eq!(
            storage.query_links(None, Some(source), Some(target)),
            vec![GenericLink::new(id, source, target)]
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn concrete_pair_query_returns_every_duplicate_in_an_undecorated_store() {
    let mut storage = heap::<u32>();
    let point = storage.create_link(0, 0).unwrap();
    let first = storage.create_link(point, point).unwrap();
    let second = storage.create_link(point, point).unwrap();
    assert_ne!(first, second);
    let mut actual = storage.query_links(None, Some(point), Some(point));
    actual.sort_unstable_by_key(|row| row.index);
    assert_eq!(
        actual,
        vec![
            GenericLink::new(first, point, point),
            GenericLink::new(second, point, point)
        ]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn reservation_reports_intermediate_rows_and_recycles_only_free_addresses() {
    let mut storage = heap::<u32>();
    let mut events = Vec::new();
    assert_eq!(
        storage
            .ensure_link_created_observed(3, &mut |before, after| events.push((before, after)))
            .unwrap(),
        3
    );
    assert_eq!(
        events,
        (1..=3)
            .map(|id| (GenericLink::null(), GenericLink::new(id, 0, 0)))
            .collect::<Vec<_>>()
    );
    events.clear();
    storage
        .ensure_link_created_observed(3, &mut |before, after| events.push((before, after)))
        .unwrap();
    assert!(events.is_empty(), "existing reservation has no row effects");
    storage.delete_link(2).unwrap();
    assert_eq!(storage.ensure_link_created(5).unwrap(), 5);
    assert_eq!(storage.links_count(), 5);
    assert_eq!(storage.create_link(0, 0).unwrap(), 6);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn missing_updates_and_deletes_return_typed_errors_without_callbacks() {
    let mut storage = heap::<u32>();
    storage.create_link(0, 0).unwrap();
    let baseline = storage.all_links();
    assert!(matches!(
        storage.update_link(9, 1, 1),
        Err(LinkError::NotFound(9))
    ));
    assert!(matches!(
        storage.delete_link(9),
        Err(LinkError::NotFound(9))
    ));
    let mut events = Vec::new();
    assert!(matches!(
        storage.update_link_observed(9, 1, 1, &mut |a, b| events.push((a, b))),
        Err(LinkError::NotFound(9))
    ));
    assert!(matches!(
        storage.delete_link_observed(9, &mut |a, b| events.push((a, b))),
        Err(LinkError::NotFound(9))
    ));
    assert!(events.is_empty());
    assert_eq!(storage.all_links(), baseline);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn observations_reconstruct_the_actual_effects_of_uniqueness_and_cascades() {
    for merge in [false, true] {
        let mut storage = heap::<u32>().with_automatic_uniqueness_and_usages_resolution();
        let point = storage.create_link(0, 0).unwrap();
        let survivor = storage.create_link(point, point).unwrap();
        let redundant = storage.create_link(point, survivor).unwrap();
        let usage = storage.create_link(redundant, redundant).unwrap();
        let baseline = storage.all_links();
        let mut events = Vec::new();
        let mut observe = |before, after| events.push((before, after));
        let returned = if merge {
            storage.update_link_observed(redundant, point, point, &mut observe)
        } else {
            storage.delete_link_observed(redundant, &mut observe)
        }
        .unwrap();
        assert_eq!(returned, GenericLink::new(redundant, point, survivor));
        assert!(!storage.link_exists(redundant));
        if merge {
            assert_eq!(
                storage.get_link(usage),
                Some(GenericLink::new(usage, survivor, survivor))
            );
        } else {
            assert!(!storage.link_exists(usage));
        }
        let mut reconstructed: std::collections::BTreeMap<_, _> =
            baseline.into_iter().map(|row| (row.index, row)).collect();
        for (before, after) in events {
            let index = if before.index == 0 {
                after.index
            } else {
                before.index
            };
            assert_eq!(
                reconstructed.get(&index).copied().unwrap_or_default(),
                before
            );
            if after.index == 0 {
                reconstructed.remove(&index);
            } else {
                reconstructed.insert(index, after);
            }
        }
        assert_eq!(
            storage.all_links(),
            reconstructed.into_values().collect::<Vec<_>>()
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn create_observations_distinguish_reuse_from_a_new_allocation() {
    let mut storage = heap::<u32>().with_automatic_uniqueness_and_usages_resolution();
    let point = storage.create_link(0, 0).unwrap();
    let first = storage.create_link(point, point).unwrap();
    let baseline = storage.all_links();
    let mut events = Vec::new();
    assert_eq!(
        storage
            .create_link_observed(point, point, &mut |before, after| events
                .push((before, after)))
            .unwrap(),
        first
    );
    assert_eq!(storage.all_links(), baseline);
    assert!(
        !events.is_empty(),
        "resolved temporary allocation must be observed"
    );
    let mut fresh = Vec::new();
    let created = storage
        .create_link_observed(first, point, &mut |a, b| fresh.push((a, b)))
        .unwrap();
    assert_ne!(created, first);
    assert!(fresh
        .iter()
        .any(|(_, after)| *after == GenericLink::new(created, first, point)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn caller_owned_heap_keeps_identity_through_mapping_and_extraction() {
    let mut storage = heap::<u32>();
    assert_eq!(storage.path(), None);
    assert!(storage.held_lock().is_none());
    assert!(matches!(storage.lock_shared(), Err(LinkError::Lock(_))));
    assert!(matches!(storage.lock_exclusive(), Err(LinkError::Lock(_))));
    let point = Doublets::create_link(storage.store_mut(), 0, 0).unwrap();
    assert_eq!(
        Doublets::get_link(storage.store(), point).unwrap().index,
        point
    );
    storage.flush().unwrap();
    storage.reload().unwrap();
    assert!(!storage.has_external_changes().unwrap());
    let mut mapped = storage.map_store(DecoratorsExt::with_inner_reference_existence_validation);
    assert_eq!(mapped.path(), None);
    assert!(mapped.create_link(point, point).is_ok());
    assert!(mapped.create_link(point, 4321).is_err());
    let extracted = mapped.into_store();
    assert!(Doublets::get_link(&extracted, point).is_some());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn heap_adoption_preserves_u64_and_usize_endpoint_values() {
    fn verify<T: LinkReference>(source: T, target: T) {
        let mut storage = heap::<T>();
        let id = storage.create_link(source, target).unwrap();
        assert_eq!(
            storage.get_link(id),
            Some(GenericLink::new(id, source, target))
        );
        assert_eq!(storage.search_link(source, target), Some(id));
        assert_eq!(
            storage.delete_link(id).unwrap(),
            GenericLink::new(id, source, target)
        );
    }
    // The endpoint exceeds u32 even when usize itself is 32 bits in WASM.
    verify::<u64>(u32::MAX as u64 + 17, u32::MAX as u64 + 19);
    verify::<usize>(usize::MAX / 4, usize::MAX / 4 - 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn transaction_rollback_restores_every_heap_cascade_and_allows_following_commit() {
    for merge in [false, true] {
        let storage = heap::<u32>().with_automatic_uniqueness_and_usages_resolution();
        let log = NamedTypesDecorator::from_link_storages(
            LinkStorage::in_memory(false),
            LinkStorage::in_memory(false),
        );
        let mut tx = GenericTransactionsDecorator::new(
            storage,
            log,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap();
        let a = tx.create(0, 0).unwrap();
        let b = tx.create(a, a).unwrap();
        let c = tx.create(b, b).unwrap();
        let baseline = tx.inner().all_links();
        tx.begin_transaction().unwrap();
        let mut events = Vec::new();
        if merge {
            tx.update_observed(b, b, b, &mut |a, b| events.push((a, b)))
                .unwrap();
        } else {
            tx.delete_observed(b, &mut |a, b| events.push((a, b)))
                .unwrap();
        }
        assert!(events.iter().any(|(before, _)| before.index == c));
        tx.rollback().unwrap();
        assert_eq!(tx.inner().all_links(), baseline);
        tx.begin_transaction().unwrap();
        let following = tx.create(c, a).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            tx.inner().get_link(following),
            Some(GenericLink::new(following, c, a))
        );
        assert_eq!(tx.last_logged_sequence(), tx.applied_sequence());
    }
}

/// wasm32-unknown-unknown has no native filesystem. Public native constructors
/// must report that capability failure through the existing typed error API.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn filesystem_constructors_report_typed_capability_errors_in_wasm() {
    let path = "unsupported.doublets";
    assert!(matches!(
        DoubletsStorage::<u32, _>::open(path),
        Err(LinkError::Io(_))
    ));
    assert!(matches!(
        DoubletsStorage::<u32, _>::open_shared(path),
        Err(LinkError::Io(_))
    ));
    assert!(matches!(
        DoubletsStorage::<u32, _>::open_exclusive(path),
        Err(LinkError::Io(_))
    ));
    assert!(matches!(
        DoubletsStorage::<u32, _>::try_open_exclusive(path),
        Err(LinkError::Io(_))
    ));
    assert!(matches!(
        DoubletsStorage::wrap_at(heap::<u32>().into_store(), path),
        Err(LinkError::Io(_))
    ));
}
