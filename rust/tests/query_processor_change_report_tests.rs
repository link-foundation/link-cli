//! A query reports every link it creates, names or redefines, so the
//! `--changes` output tells the whole truth about what the store holds now.

use anyhow::Result;
use link_cli::{Changes, Link, NamedTypeLinks, NamedTypesDecorator, QueryProcessor};
use tempfile::NamedTempFile;

struct Store {
    links: NamedTypesDecorator,
    processor: QueryProcessor,
    _files: (NamedTempFile, NamedTempFile),
}

impl Store {
    fn new() -> Result<Self> {
        let database = NamedTempFile::new()?;
        let names = NamedTempFile::new()?;
        let links =
            NamedTypesDecorator::with_names_database_path(database.path(), names.path(), false)?;
        let processor = QueryProcessor::new(false).with_auto_create_missing_references(true);
        Ok(Self {
            links,
            processor,
            _files: (database, names),
        })
    }

    fn query(&mut self, query: &str) -> Result<Changes> {
        Ok(sorted(
            self.processor.process_query(&mut self.links, query)?,
        ))
    }

    fn named(&mut self, name: &str) -> Result<u32> {
        Ok(self
            .links
            .get_by_name(name)?
            .unwrap_or_else(|| panic!("{name} should exist")))
    }

    fn point(&mut self, name: &str) -> Result<Link> {
        let index = self.named(name)?;
        Ok(Link::new(index, index, index))
    }
}

fn sorted(mut changes: Changes) -> Changes {
    changes.sort_by_key(|(before, after)| (before.map(|l| l.index), after.map(|l| l.index)));
    changes
}

fn created(link: Link) -> (Option<Link>, Option<Link>) {
    (None, Some(link))
}

#[test]
fn a_numeric_point_link_is_reported_created() -> Result<()> {
    let mut store = Store::new()?;
    assert_eq!(
        store.query("() ((2: 2 2))")?,
        vec![created(Link::new(2, 2, 2))]
    );
    Ok(())
}

#[test]
fn a_named_point_link_is_reported_created() -> Result<()> {
    let mut store = Store::new()?;
    let changes = store.query("() ((a: a a))")?;
    assert_eq!(changes, vec![created(store.point("a")?)]);
    Ok(())
}

#[test]
fn a_named_leaf_created_on_the_way_is_reported_created() -> Result<()> {
    let mut store = Store::new()?;
    let changes = store.query("() ((c: c d))")?;
    let (c, d) = (store.named("c")?, store.named("d")?);
    assert_eq!(
        changes,
        sorted(vec![
            created(Link::new(c, c, d)),
            created(Link::new(d, d, d))
        ])
    );
    Ok(())
}

#[test]
fn every_named_leaf_of_a_composite_is_reported_created() -> Result<()> {
    let mut store = Store::new()?;
    let changes = store.query("() ((child: father mother))")?;
    let (child, father, mother) = (
        store.named("child")?,
        store.named("father")?,
        store.named("mother")?,
    );
    assert_eq!(
        changes,
        sorted(vec![
            created(Link::new(child, father, mother)),
            created(Link::new(father, father, father)),
            created(Link::new(mother, mother, mother)),
        ])
    );
    assert_eq!(store.links.all_links().len(), 3);
    Ok(())
}

#[test]
fn creating_an_existing_name_redefines_that_link() -> Result<()> {
    let mut store = Store::new()?;
    store.query("() ((child: father mother))")?;
    let (child, father, mother) = (
        store.named("child")?,
        store.named("father")?,
        store.named("mother")?,
    );

    assert_eq!(
        store.query("() ((child: mother father))")?,
        vec![(
            Some(Link::new(child, father, mother)),
            Some(Link::new(child, mother, father))
        )]
    );
    assert_eq!(store.links.all_links().len(), 3);
    Ok(())
}

#[test]
fn a_new_name_for_an_existing_doublet_names_it_instead_of_copying_it() -> Result<()> {
    let mut store = Store::new()?;
    store.query("() ((a: a a))")?;
    let a = store.point("a")?;

    assert_eq!(store.query("() ((b: a a))")?, vec![(Some(a), Some(a))]);
    assert_eq!(store.named("b")?, a.index);
    assert_eq!(store.links.all_links(), vec![a]);
    Ok(())
}

#[test]
fn a_numeric_reference_is_reported_created_as_the_point_link() -> Result<()> {
    let mut store = Store::new()?;
    assert_eq!(
        store.query("() ((3: 3 2))")?,
        vec![created(Link::new(2, 2, 2)), created(Link::new(3, 3, 2))]
    );
    Ok(())
}

/// Doublets are unique, so the point link `(2: 2 2)` and the defined
/// `(3: 2 2)` cannot both exist: the reference is left empty.
#[test]
fn a_reference_to_the_pair_a_link_defines_is_reported_created_empty() -> Result<()> {
    let mut store = Store::new()?;
    assert_eq!(
        store.query("() ((3: 2 2))")?,
        vec![created(Link::new(2, 0, 0)), created(Link::new(3, 2, 2))]
    );
    assert_eq!(
        store.links.all_links(),
        vec![Link::new(2, 0, 0), Link::new(3, 2, 2)]
    );
    Ok(())
}
