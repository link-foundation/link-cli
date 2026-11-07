//! The edges of stored triggers: records the schema does not make, parts of a
//! decorator handed out and writes that fire nothing.

use anyhow::Result;
use link_cli::{
    Link, LinkStorage, NamedTypeLinks, PersistentTransformationDecorator,
    PersistentTransformationKind as Kind, PersistentTransformationQuery, TriggerStore,
};

const REWRITE: &str = "(((1: 1 1)) ((1: 1 2)))";
const OTHER: &str = "(((5: 5 5)) ((5: 5 6)))";

fn embedded() -> PersistentTransformationDecorator<LinkStorage> {
    PersistentTransformationDecorator::embedded(LinkStorage::in_memory(false), false)
}

#[test]
fn a_query_needs_both_of_its_parts() {
    for query in ["(1)", "((1: 1 1))", "()"] {
        assert!(
            PersistentTransformationQuery::parse(query).is_err(),
            "{query}"
        );
    }
    assert!(PersistentTransformationQuery::parse(REWRITE).is_ok());
    let bare = PersistentTransformationQuery::parse("((1: 1 1)) ((1: 1 2))").unwrap();
    assert_eq!(bare, PersistentTransformationQuery::parse(REWRITE).unwrap());
}

#[test]
fn a_sidecar_that_cannot_open_is_an_error() {
    assert!(TriggerStore::sidecar("/dev/null/triggers.links", false).is_err());
}

#[test]
fn a_decorator_hands_out_its_parts() {
    let mut decorator = embedded().with_auto_create_missing_references(true);
    assert!(decorator.auto_create_missing_references());
    decorator.set_auto_create_missing_references(false);
    assert!(!decorator.auto_create_missing_references());
    decorator.inner_mut().create(0, 0);
    assert_eq!(decorator.inner().all().len(), 1);
    assert!(matches!(decorator.trigger_store(), TriggerStore::Embedded));
    assert!(matches!(
        decorator.trigger_store_mut(),
        TriggerStore::Embedded
    ));
    let (links, store) = decorator.into_parts();
    assert_eq!(links.all().len(), 1);
    assert!(matches!(store, TriggerStore::Embedded));
}

#[test]
fn only_a_whole_record_reads_as_a_trigger() -> Result<()> {
    let mut decorator = embedded();
    let root = decorator.store_trigger(Kind::Always, REWRITE)?;
    let kept = decorator.triggers()?;
    assert_eq!(kept.len(), 1);
    let link = decorator.inner_mut().get_link(root).unwrap();
    let (always, payload) = (link.source, link.target);
    let record = decorator.inner_mut().get_link(payload).unwrap();
    let links = decorator.inner_mut();

    // No payload, a payload of nothing known, and of the wrong records.
    links.create(always, 9_999);
    let unknown = links.create(9_998, 9_997);
    links.create(always, unknown);
    let half = links.create(record.source, 9_996);
    links.create(always, half);
    let swapped = links.create(record.target, record.source);
    links.create(always, swapped);
    // Records of the right schema whose text is not the text of their part.
    let plain = links.create(0, 0);
    let schema_condition = links.get_link(record.source).unwrap().source;
    let schema_substitution = links.get_link(record.target).unwrap().source;
    let condition = links.create(schema_condition, plain);
    let substitution = links.create(schema_substitution, plain);
    let nameless = links.create(condition, substitution);
    links.create(always, nameless);

    assert_eq!(decorator.triggers()?, kept);
    Ok(())
}

#[test]
fn removing_triggers_keeps_the_others_and_finds_each_kind() -> Result<()> {
    let mut decorator = embedded();
    let rewrite = decorator.store_trigger(Kind::Always, REWRITE)?;
    decorator.store_trigger(Kind::Once, REWRITE)?;
    let other = decorator.store_trigger(Kind::Always, OTHER)?;
    assert_eq!(decorator.remove_triggers("(((9: 9 9)) ((9: 9 8)))")?, 0);
    assert_eq!(decorator.remove_triggers(REWRITE)?, 2);
    assert!(!decorator.inner_mut().exists(rewrite));
    let left: Vec<u32> = decorator.triggers()?.iter().map(|t| t.root).collect();
    assert_eq!(left, [other]);
    Ok(())
}

#[test]
fn a_write_that_changes_nothing_fires_nothing() -> Result<()> {
    let mut decorator = embedded().with_auto_create_missing_references(true);
    let first = decorator.ensure_created(1);
    assert_eq!(decorator.ensure_created(first), first);
    let pair = decorator.get_or_create(first, first);
    assert_eq!(decorator.get_or_create(first, first), pair);
    // A trigger that fires would add this link.
    decorator.store_trigger(Kind::Always, "(() ((70: 70 70)))")?;
    let before = decorator.all_links().len();
    assert_eq!(decorator.get_or_create(first, first), pair);
    assert_eq!(decorator.ensure_created(first), first);
    assert_eq!(decorator.all_links().len(), before, "no trigger ran");
    assert_eq!(decorator.search(first, first), Some(pair));
    assert_eq!(
        decorator.query_links(Some(pair), None, None),
        [Link::new(pair, first, first)]
    );
    Ok(())
}
