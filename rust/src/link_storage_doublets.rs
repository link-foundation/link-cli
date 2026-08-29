//! Bridge between the CLI's [`LinkStorage`] and the upstream `doublets` traits.
//!
//! Implementing [`Links`] and [`Doublets`] for [`LinkStorage`] lets this crate
//! reuse the `doublets::decorators` layer instead of re-implementing uniqueness
//! resolution and cascading deletion by hand. It is the direct analogue of what
//! the C# implementation does in
//! `Foundation.Data.Doublets.Cli.NamedTypesDecorator.MakeLinks`:
//!
//! ```csharp
//! var links = new UnitedMemoryLinks<TLinkAddress>(databaseFilename);
//! return links.DecorateWithAutomaticUniquenessAndUsagesResolution();
//! ```
//!
//! [`LinkStorage`] plays the `UnitedMemoryLinks` role — a plain store with no
//! policy of its own — and
//! [`DecoratorsExt::with_automatic_uniqueness_and_usages_resolution`](doublets::decorators::DecoratorsExt::with_automatic_uniqueness_and_usages_resolution)
//! supplies the policy.
//!
//! The impls are also part of the public API on purpose: any embedder can now
//! stack arbitrary upstream decorators (or its own) onto a [`LinkStorage`], and
//! pass one anywhere a `doublets::Doublets<u32>` is expected.

use std::sync::OnceLock;

use doublets::data::{Flow, LinksConstants, ReadHandler, WriteHandler};
use doublets::{Doublets, Error, Link as DoubletsLink, Links};

use crate::link::Link;
use crate::link_storage::LinkStorage;

/// The [`LinksConstants`] every [`LinkStorage`] reports.
///
/// [`LinkStorage`] addresses links with plain `u32` values and has no external
/// reference range, so the default internal-only constants apply: `null` is
/// `0`, and the service values (`any`, `itself`, ...) live just below
/// [`u32::MAX`], outside the range the storage ever allocates.
pub fn link_storage_constants() -> &'static LinksConstants<u32> {
    static CONSTANTS: OnceLock<LinksConstants<u32>> = OnceLock::new();
    CONSTANTS.get_or_init(LinksConstants::new)
}

fn as_doublets_link(link: &Link) -> DoubletsLink<u32> {
    DoubletsLink::new(link.index, link.source, link.target)
}

/// Reads one part out of a raw query slice, defaulting to `null` when the slice
/// is shorter — the same rule `doublets` uses internally.
fn part(query: &[u32], index: usize) -> u32 {
    query.get(index).copied().unwrap_or(0)
}

/// Every link, ordered by address.
///
/// Mirrors `each_core(handler, &[])` in the upstream unit store, which walks
/// allocated addresses from `1` upwards.
fn all(storage: &LinkStorage) -> Vec<Link> {
    let mut matched: Vec<Link> = storage.all().into_iter().copied().collect();
    matched.sort_by_key(|link| link.index);
    matched
}

/// The `(any, source, target)` case, reproducing the *index* semantics of the
/// upstream unit store rather than a plain scan.
///
/// In `doublets` a link is only reachable through the `(source, target)`
/// lookups while it is attached to the source and target trees, and
/// [`mem::united::Store::update_links`] only attaches the parts that are not
/// `null`:
///
/// ```rust,ignore
/// if place.source != T::from_byte(0) { unsafe { self.attach_source(index); } }
/// if place.target != T::from_byte(0) { unsafe { self.attach_target(index); } }
/// ```
///
/// A freshly created — or deliberately blanked — link therefore never answers a
/// `(source, target)` query, which is exactly what keeps
/// [`UniquenessResolver`](doublets::decorators::UniquenessResolver) from merging
/// all the not-yet-filled links with each other. Matching that rule here is what
/// makes the decorators behave the same on top of a [`LinkStorage`].
fn by_pattern(storage: &LinkStorage, source: u32, target: u32) -> Vec<Link> {
    let constants = link_storage_constants();
    let (any, null) = (constants.any, constants.null);

    match (source == any, target == any) {
        (true, true) => all(storage),
        // `targets.each_usages(target)`: the tree rooted at a null target is empty.
        (true, false) if target == null => Vec::new(),
        (true, false) => {
            let mut matched: Vec<Link> = storage
                .all()
                .into_iter()
                .filter(|link| link.target == target)
                .copied()
                .collect();
            matched.sort_by_key(|link| link.index);
            matched
        }
        // `sources.each_usages(source)`: the tree rooted at a null source is empty.
        (false, true) if source == null => Vec::new(),
        (false, true) => {
            let mut matched: Vec<Link> = storage
                .all()
                .into_iter()
                .filter(|link| link.source == source)
                .copied()
                .collect();
            matched.sort_by_key(|link| link.index);
            matched
        }
        (false, false) if source == null || target == null => Vec::new(),
        // `sources.search(source, target)` yields at most one link; the lowest
        // address wins so the answer never depends on hash map ordering.
        (false, false) => storage
            .all()
            .into_iter()
            .filter(|link| link.source == source && link.target == target)
            .min_by_key(|link| link.index)
            .copied()
            .into_iter()
            .collect(),
    }
}

/// Links matching `query`, ordered by address so that every traversal (and
/// therefore every cascade) is deterministic regardless of hash map ordering.
///
/// This is a faithful port of `each_core` in the upstream unit store, including
/// its handling of one- and two-element queries and of the `any` constant.
/// Query shapes the raw interface does not define match nothing.
fn matching(storage: &LinkStorage, query: &[u32]) -> Vec<Link> {
    let any = link_storage_constants().any;

    match *query {
        [] => all(storage),
        [index] if index == any => all(storage),
        [index] => storage.get(index).copied().into_iter().collect(),
        [index, value] if index == any && value == any => all(storage),
        [index, value] if index == any => {
            // Upstream unions the two usage trees without deduplicating, so a
            // link that both starts and ends at `value` is visited twice.
            let mut matched = by_pattern(storage, value, any);
            matched.extend(by_pattern(storage, any, value));
            matched
        }
        [index, value] => storage
            .get(index)
            .filter(|link| value == any || link.source == value || link.target == value)
            .copied()
            .into_iter()
            .collect(),
        [index, source, target] if index == any => by_pattern(storage, source, target),
        [index, source, target] => storage
            .get(index)
            .filter(|link| {
                (source == any || link.source == source) && (target == any || link.target == target)
            })
            .copied()
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

impl Links<u32> for LinkStorage {
    fn constants(&self) -> &LinksConstants<u32> {
        link_storage_constants()
    }

    fn count_links(&self, query: &[u32]) -> u32 {
        matching(self, query).len() as u32
    }

    fn create_links(
        &mut self,
        _query: &[u32],
        handler: WriteHandler<'_, u32>,
    ) -> Result<Flow, Error<u32>> {
        let index = self.create(0, 0);
        let created = Link::new(index, 0, 0);
        Ok(handler(DoubletsLink::nothing(), as_doublets_link(&created)))
    }

    fn each_links(&self, query: &[u32], handler: ReadHandler<'_, u32>) -> Flow {
        for link in matching(self, query) {
            if handler(as_doublets_link(&link)) == Flow::Break {
                return Flow::Break;
            }
        }
        Flow::Continue
    }

    fn update_links(
        &mut self,
        query: &[u32],
        change: &[u32],
        handler: WriteHandler<'_, u32>,
    ) -> Result<Flow, Error<u32>> {
        let index = part(query, 0);
        let source = part(change, 1);
        let target = part(change, 2);
        let before = self
            .update_raw(index, source, target)
            .map_err(|_| Error::NotExists(index))?;
        let after = Link::new(index, source, target);
        Ok(handler(as_doublets_link(&before), as_doublets_link(&after)))
    }

    fn delete_links(
        &mut self,
        query: &[u32],
        handler: WriteHandler<'_, u32>,
    ) -> Result<Flow, Error<u32>> {
        let index = part(query, 0);
        let before = self
            .delete_raw(index)
            .map_err(|_| Error::NotExists(index))?;
        Ok(handler(as_doublets_link(&before), DoubletsLink::nothing()))
    }
}

impl Doublets<u32> for LinkStorage {
    fn get_link(&self, index: u32) -> Option<DoubletsLink<u32>> {
        self.get(index).map(as_doublets_link)
    }
}

/// `doublets` ships no blanket implementation for references, so decorating a
/// borrowed store (instead of moving it into the decorator) needs this
/// forwarding impl. It is what lets [`LinkStorage`] decorate itself for the
/// duration of a single operation.
impl Links<u32> for &mut LinkStorage {
    fn constants(&self) -> &LinksConstants<u32> {
        (**self).constants()
    }

    fn count_links(&self, query: &[u32]) -> u32 {
        (**self).count_links(query)
    }

    fn create_links(
        &mut self,
        query: &[u32],
        handler: WriteHandler<'_, u32>,
    ) -> Result<Flow, Error<u32>> {
        (**self).create_links(query, handler)
    }

    fn each_links(&self, query: &[u32], handler: ReadHandler<'_, u32>) -> Flow {
        (**self).each_links(query, handler)
    }

    fn update_links(
        &mut self,
        query: &[u32],
        change: &[u32],
        handler: WriteHandler<'_, u32>,
    ) -> Result<Flow, Error<u32>> {
        (**self).update_links(query, change, handler)
    }

    fn delete_links(
        &mut self,
        query: &[u32],
        handler: WriteHandler<'_, u32>,
    ) -> Result<Flow, Error<u32>> {
        (**self).delete_links(query, handler)
    }
}

impl Doublets<u32> for &mut LinkStorage {
    fn get_link(&self, index: u32) -> Option<DoubletsLink<u32>> {
        (**self).get_link(index)
    }
}
