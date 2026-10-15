//! Optional secondary-index decorator for the in-memory links map.
//!
//! Enabled by [`LinkStorage::with_indexed_lookups`](crate::LinkStorage::with_indexed_lookups).
//! The undecorated map remains available for algorithm comparisons. Indexes
//! are maintained at the raw insertion/removal boundary, including writes made
//! by upstream uniqueness and usages-resolution decorators.

use std::collections::{BTreeSet, HashMap};
use std::ops::Deref;

use crate::Link;

type Addresses<K> = HashMap<K, BTreeSet<u32>>;

/// Adds exact-pair, source and target indexes to a links map.
///
/// Use the `with_indexed_lookups` builders on [`crate::LinkStorage`] or
/// [`crate::NamedTypesDecorator`] to compose this decorator. Duplicate pairs
/// retain every address; searches select the lowest address, just as scans do.
#[derive(Default)]
pub struct IndexedLinksDecorator {
    inner: HashMap<u32, Link>,
    pairs: Addresses<(u32, u32)>,
    sources: Addresses<u32>,
    targets: Addresses<u32>,
}

impl IndexedLinksDecorator {
    fn insert(&mut self, link: Link) -> Option<Link> {
        let previous = self.remove(link.index);
        self.pairs
            .entry((link.source, link.target))
            .or_default()
            .insert(link.index);
        self.sources
            .entry(link.source)
            .or_default()
            .insert(link.index);
        self.targets
            .entry(link.target)
            .or_default()
            .insert(link.index);
        self.inner.insert(link.index, link);
        previous
    }

    fn remove(&mut self, id: u32) -> Option<Link> {
        let link = self.inner.remove(&id)?;
        remove_address(&mut self.pairs, (link.source, link.target), id);
        remove_address(&mut self.sources, link.source, id);
        remove_address(&mut self.targets, link.target, id);
        Some(link)
    }

    pub(crate) fn search(&self, source: u32, target: u32) -> Option<u32> {
        self.pairs.get(&(source, target))?.first().copied()
    }

    pub(crate) fn query(
        &self,
        index: Option<u32>,
        source: Option<u32>,
        target: Option<u32>,
    ) -> Vec<&Link> {
        if let Some(id) = index {
            return self
                .inner
                .get(&id)
                .filter(|link| {
                    source.is_none_or(|value| link.source == value)
                        && target.is_none_or(|value| link.target == value)
                })
                .into_iter()
                .collect();
        }
        let addresses = match (source, target) {
            (Some(source), Some(target)) => self.pairs.get(&(source, target)),
            (Some(source), None) => self.sources.get(&source),
            (None, Some(target)) => self.targets.get(&target),
            (None, None) => return self.inner.values().collect(),
        };
        addresses
            .into_iter()
            .flatten()
            .filter_map(|id| self.inner.get(id))
            .collect()
    }
}

fn remove_address<K: std::hash::Hash + Eq>(index: &mut Addresses<K>, key: K, address: u32) {
    if let Some(addresses) = index.get_mut(&key) {
        addresses.remove(&address);
        if addresses.is_empty() {
            index.remove(&key);
        }
    }
}

/// Read access is shared, but every write goes through `insert`, `remove` or
/// `clear`: callers cannot obtain a mutable map reference and stale the indexes.
pub(crate) enum LinksMap {
    Plain(HashMap<u32, Link>),
    Indexed(IndexedLinksDecorator),
}

impl Default for LinksMap {
    fn default() -> Self {
        Self::Plain(HashMap::new())
    }
}

impl Deref for LinksMap {
    type Target = HashMap<u32, Link>;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Plain(map) => map,
            Self::Indexed(index) => &index.inner,
        }
    }
}

impl LinksMap {
    pub(crate) fn enable_index(&mut self) {
        if matches!(self, Self::Plain(_)) {
            let links = std::mem::take(self);
            let mut indexed = IndexedLinksDecorator::default();
            for link in links.values() {
                indexed.insert(*link);
            }
            *self = Self::Indexed(indexed);
        }
    }

    pub(crate) fn indexed(&self) -> Option<&IndexedLinksDecorator> {
        match self {
            Self::Indexed(index) => Some(index),
            _ => None,
        }
    }

    pub(crate) fn insert(&mut self, id: u32, link: Link) -> Option<Link> {
        debug_assert_eq!(id, link.index);
        match self {
            Self::Plain(map) => map.insert(id, link),
            Self::Indexed(index) => index.insert(link),
        }
    }

    pub(crate) fn remove(&mut self, id: &u32) -> Option<Link> {
        match self {
            Self::Plain(map) => map.remove(id),
            Self::Indexed(index) => index.remove(*id),
        }
    }

    pub(crate) fn clear(&mut self) {
        match self {
            Self::Plain(map) => map.clear(),
            Self::Indexed(index) => *index = IndexedLinksDecorator::default(),
        }
    }
}
