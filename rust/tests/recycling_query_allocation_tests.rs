//! Reference planning must follow execution on the legacy recycling allocator.
use link_cli::{Link, LinkStorage, NamedTypeLinks, NamedTypesDecorator, QueryProcessor};

fn named() -> NamedTypesDecorator {
    NamedTypesDecorator::from_link_storages(
        LinkStorage::in_memory(false),
        LinkStorage::in_memory(false),
    )
}

macro_rules! with_stores {
    ($check:ident) => {
        for indexed in [false, true] {
            let data = LinkStorage::in_memory(false);
            let mut plain = if indexed {
                data.with_indexed_lookups()
            } else {
                data
            };
            $check(&mut plain);
            let data = LinkStorage::in_memory(false);
            let names = LinkStorage::in_memory(false);
            let links = NamedTypesDecorator::from_link_storages(data, names);
            let mut named = if indexed {
                links.with_indexed_lookups()
            } else {
                links
            };
            $check(&mut named);
        }
    };
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn explicit_definition_changes_the_following_recycled_allocation() {
    fn check(links: &mut impl NamedTypeLinks) {
        QueryProcessor::new(false)
            .process_query(links, "() ((1: 1 1) (2 2))")
            .unwrap();
        assert_eq!(
            links.all_links(),
            vec![Link::new(1, 1, 1), Link::new(2, 2, 2)]
        );
    }
    with_stores!(check);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn named_leaf_changes_the_following_recycled_allocation() {
    fn check(links: &mut impl NamedTypeLinks) {
        QueryProcessor::new(false)
            .process_query(links, "() ((foo: foo foo) (2 2))")
            .unwrap();
        assert_eq!(
            links.all_links(),
            vec![Link::new(1, 1, 1), Link::new(2, 2, 2)]
        );
        assert_eq!(links.get_by_name("foo").unwrap(), Some(1));
    }
    with_stores!(check);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn high_definition_does_not_authorize_an_uncreated_low_reference() {
    fn check(links: &mut impl NamedTypeLinks) {
        assert!(QueryProcessor::new(false)
            .process_query(links, "() ((100: 100 100) (1 1))")
            .is_err());
        assert!(links.all_links().is_empty());
        assert_eq!(links.try_create(0, 0).unwrap(), 1);
    }
    with_stores!(check);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn high_definition_allows_the_actual_recycled_reference() {
    fn check(links: &mut impl NamedTypeLinks) {
        QueryProcessor::new(false)
            .process_query(links, "() ((100: 100 100) (99 99))")
            .unwrap();
        assert_eq!(
            links.all_links(),
            vec![Link::new(99, 99, 99), Link::new(100, 100, 100)]
        );
        assert_eq!(links.try_create(0, 0).unwrap(), 98);
    }
    with_stores!(check);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn rejected_preview_preserves_names_live_rows_and_free_list_order() {
    fn check(links: &mut impl NamedTypeLinks) {
        for id in 1..=4 {
            assert_eq!(links.try_create(id, id).unwrap(), id);
        }
        links.set_name(1, "retained λ").unwrap();
        links.delete(2).unwrap();
        links.delete(3).unwrap();
        let before = links.all_links();
        assert!(QueryProcessor::new(false)
            .process_query(links, "() ((100: 100 100) (2 2))")
            .is_err());
        assert_eq!(links.all_links(), before);
        assert_eq!(links.get_name(1).unwrap().as_deref(), Some("retained λ"));
        assert_eq!(links.try_create(0, 0).unwrap(), 3);
        assert_eq!(links.try_create(0, 0).unwrap(), 2);
    }
    with_stores!(check);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn nested_pair_reuse_does_not_authorize_an_extra_reference() {
    fn check(links: &mut impl NamedTypeLinks) {
        let processor = QueryProcessor::new(false);
        assert!(processor
            .process_query(links, "() ((foo: (1 1) (1 1)) (3 3))")
            .is_err());
        assert!(links.all_links().is_empty());
        processor
            .process_query(links, "() ((foo: (1 1) (1 1)))")
            .unwrap();
        assert_eq!(links.all_links(), vec![Link::new(1, 1, 1)]);
        assert_eq!(links.try_create(0, 0).unwrap(), 2);
    }
    with_stores!(check);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn transaction_preview_writes_no_journal_and_preserves_rollback() {
    use link_cli::{CommitMode, LogRetentionPolicy, TransactionsDecorator};
    let mut tx = TransactionsDecorator::new(
        named(),
        named(),
        LogRetentionPolicy::default(),
        CommitMode::default(),
        false,
    )
    .unwrap();
    let processor = QueryProcessor::new(false);
    assert!(processor
        .process_query(&mut tx, "() ((100: 100 100) (1 1))")
        .is_err());
    assert!(tx.log().is_empty());
    tx.begin_transaction().unwrap();
    processor
        .process_query(&mut tx, "() ((1: 1 1) (2 2))")
        .unwrap();
    assert_eq!(
        NamedTypeLinks::all_links(&mut tx),
        vec![Link::new(1, 1, 1), Link::new(2, 2, 2)]
    );
    assert_eq!(
        tx.log().len(),
        3,
        "only explicit creation/update and anonymous creation are logged"
    );
    tx.rollback().unwrap();
    assert!(NamedTypeLinks::all_links(&mut tx).is_empty());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn version_preview_preserves_history_and_following_checkout() {
    use link_cli::{
        CommitMode, LogRetentionPolicy, TransactionsDecorator, VersionControlDecorator,
    };
    let tx = TransactionsDecorator::new(
        named(),
        named(),
        LogRetentionPolicy::default(),
        CommitMode::default(),
        false,
    )
    .unwrap();
    let mut vc = VersionControlDecorator::new(tx, named(), false).unwrap();
    let processor = QueryProcessor::new(false);
    assert!(processor
        .process_query(&mut vc, "() ((100: 100 100) (1 1))")
        .is_err());
    assert_eq!(vc.current_sequence(), 0);
    assert!(vc.transactions().log().is_empty());
    processor
        .process_query(&mut vc, "() ((1: 1 1) (2 2))")
        .unwrap();
    let head = vc.current_sequence();
    assert!(head > 0);
    let expected = vec![Link::new(1, 1, 1), Link::new(2, 2, 2)];
    assert_eq!(NamedTypeLinks::all_links(&mut vc), expected);
    vc.checkout(0).unwrap();
    assert!(NamedTypeLinks::all_links(&mut vc).is_empty());
    vc.checkout(head).unwrap();
    assert_eq!(NamedTypeLinks::all_links(&mut vc), expected);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn rejected_preview_never_writes_the_data_or_names_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("data.links");
    let names_path = NamedTypesDecorator::make_names_database_filename(&path);
    let mut links = NamedTypesDecorator::new(&path, false).unwrap();
    for id in 1..=4 {
        assert_eq!(links.try_create(id, id).unwrap(), id);
    }
    links.set_name(1, "retained λ").unwrap();
    links.delete(2).unwrap();
    links.delete(3).unwrap();
    links.save().unwrap();
    let data_before = std::fs::read(&path).unwrap();
    let names_before = std::fs::read(&names_path).unwrap();
    assert!(QueryProcessor::new(false)
        .process_query(&mut links, "() ((100: 100 100) (2 2))")
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), data_before);
    assert_eq!(std::fs::read(&names_path).unwrap(), names_before);
    assert_eq!(links.get_name(1).unwrap().as_deref(), Some("retained λ"));
    assert_eq!(links.try_create(0, 0).unwrap(), 3);
    assert_eq!(links.try_create(0, 0).unwrap(), 2);
}
