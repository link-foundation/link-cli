use link_cli::{
    GenericLink, LinkStorage, LinksStorage, LinksStorageRef, NamedTypesDecorator, PinnedTypes,
    PinnedTypesDecorator,
};

fn raw_contract<S: LinksStorage<u32> + LinksStorageRef<u32>>(store: &mut S) {
    let first = store.create_link(10, 20).unwrap();
    assert_eq!(store.ensure_link_created(9).unwrap(), 9);
    assert_eq!(store.get_or_create_link(10, 20).unwrap(), first);
    assert_eq!(store.search_link(10, 20), Some(first));
    assert!(store.link_exists(first));
    assert!(!store.link_exists(99));
    assert_eq!(store.get_link_ref(first).unwrap().source, 10);
    assert!(store.get_link_ref(99).is_none());
    assert_eq!(store.all_links().len(), 2);
    assert_eq!(store.all_link_refs().len(), 2);
    assert_eq!(
        store.query_links(None, Some(10), None),
        [GenericLink::new(first, 10, 20)]
    );
    assert_eq!(store.query_link_refs(None, Some(10), Some(20)).len(), 1);
    assert!(store.query_link_refs(None, Some(99), None).is_empty());
    assert_eq!(
        store.update_link(first, 11, 21).unwrap(),
        GenericLink::new(first, 10, 20)
    );
    assert!(store.update_link(99, 1, 2).is_err());
    let mut changes = Vec::new();
    store
        .update_link_observed(first, 12, 22, &mut |a, b| changes.push((a, b)))
        .unwrap();
    store
        .delete_link_observed(9, &mut |a, b| changes.push((a, b)))
        .unwrap();
    assert_eq!(
        changes,
        [
            (
                GenericLink::new(first, 11, 21),
                GenericLink::new(first, 12, 22)
            ),
            (GenericLink::new(9, 0, 0), GenericLink::null()),
        ]
    );
    assert!(store
        .update_link_observed(99, 0, 0, &mut |_, _| panic!("failed update callback"))
        .is_err());
    assert!(store
        .delete_link_observed(99, &mut |_, _| panic!("failed delete callback"))
        .is_err());
    assert_eq!(
        store.delete_link(first).unwrap(),
        GenericLink::new(first, 12, 22)
    );
    assert!(store.delete_link(99).is_err());
    store.flush().unwrap();
    store.reload().unwrap();
    assert!(!store.has_external_changes().unwrap());
    assert!(store.all_links().is_empty());
}

#[test]
fn raw_api_preserves_behavior_through_memory_file_pinned_and_named_adapters() {
    raw_contract(&mut LinkStorage::in_memory(false));
    let dir = tempfile::tempdir().unwrap();
    raw_contract(&mut LinkStorage::new(dir.path().join("plain"), false).unwrap());
    let mut pinned = PinnedTypesDecorator::new(dir.path().join("pinned"), true).unwrap();
    raw_contract(&mut pinned);
    assert!(pinned.into_link_storage().all().is_empty());
    let mut named = NamedTypesDecorator::new(dir.path().join("named"), false)
        .unwrap()
        .with_indexed_lookups();
    raw_contract(&mut named);
    assert!(named.pinned_types_decorator_mut().all().is_empty());
    let (links, _names) = named.into_link_storages();
    assert!(links.all().is_empty());
}

#[test]
fn file_backed_decorators_forward_external_changes_and_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinned");
    let mut pinned = PinnedTypesDecorator::new(&path, false).unwrap();
    let id = pinned.create(10, 20);
    LinksStorage::flush(&mut pinned).unwrap();
    let mut outside = LinkStorage::new(&path, false).unwrap();
    outside.create(30, 40);
    outside.save().unwrap();
    assert!(LinksStorage::has_external_changes(&pinned).unwrap());
    LinksStorage::reload(&mut pinned).unwrap();
    assert_eq!(pinned.all().len(), 2);
    assert_eq!(pinned.update(id, 11, 21).unwrap().source, 10);
    assert_eq!(pinned.query(Some(id), None, None).len(), 1);
    assert_eq!(pinned.delete(id).unwrap().source, 11);
    pinned.save().unwrap();
    let mut named = NamedTypesDecorator::new(dir.path().join("named"), false).unwrap();
    LinksStorage::flush(&mut named).unwrap();
    // A names-sidecar change must also invalidate the composed adapter.
    let mut outside_names = LinkStorage::new(named.names_links().database_path(), false).unwrap();
    outside_names.create(50, 60);
    outside_names.save().unwrap();
    assert!(LinksStorage::has_external_changes(&named).unwrap());
    LinksStorage::reload(&mut named).unwrap();
    assert_eq!(named.names_links().all().len(), 1);
    assert!(!LinksStorage::has_external_changes(&named).unwrap());
}

#[test]
fn named_flush_reports_sidecar_failure_and_can_retry_after_repair() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let names = dir.path().join("names");
    let mut named = NamedTypesDecorator::with_names_database_path(&data, &names, false).unwrap();
    named.create(10, 20);
    std::fs::create_dir(&names).unwrap();
    assert!(LinksStorage::flush(&mut named).is_err());
    assert_eq!(LinkStorage::new(&data, false).unwrap().all().len(), 1);
    std::fs::remove_dir(&names).unwrap();
    LinksStorage::flush(&mut named).unwrap();
    LinksStorage::reload(&mut named).unwrap();
    assert_eq!(named.all().len(), 1);
}

#[test]
fn pinned_triplets_validate_shape_and_reject_nondeterministic_address_reuse() {
    let mut empty = LinkStorage::in_memory(false);
    assert_eq!(
        PinnedTypes::new(&mut empty).deconstruct().unwrap(),
        (1, 2, 3)
    );
    assert_eq!(
        PinnedTypes::new(&mut empty).deconstruct().unwrap(),
        (1, 2, 3)
    );
    for (source, target) in [(2, 1), (1, 2)] {
        let mut invalid = LinkStorage::in_memory(false);
        invalid.create(source, target);
        assert!(PinnedTypes::new(&mut invalid)
            .deconstruct()
            .unwrap_err()
            .to_string()
            .contains("Unexpected link"));
    }
    let mut displaced = LinkStorage::in_memory(false);
    displaced.ensure_created(5);
    displaced.update(5, 1, 1).unwrap();
    assert!(PinnedTypes::new(&mut displaced)
        .next_type()
        .unwrap_err()
        .to_string()
        .contains("could not be created deterministically"));
}
