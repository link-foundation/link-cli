//! Triplets `(index: type from to)` stored in doublets, with an associative
//! API over any [`NamedTypeLinks`] (issue #151).
//!
//! The encoding keeps the convention of the type hierarchy of issue #148,
//! *the type of a link is its source*, and adds two marker points, `Pair`
//! (`P`) and `Nothing` (`N`, which stands for the empty part `0`):
//!
//! | triplet            | doublets                                                        |
//! |--------------------|-----------------------------------------------------------------|
//! | `(i: T 0 0)`, node | `(i: T i)`                                                      |
//! | `(i: T f t)`       | wrapper `w = (P f)`, pair `p = (w t)`, outer `(i: T p)`         |
//! | the same, again    | the outer `(T p)` is taken, so a duplicate pair `d = (p i)` and `(i: T d)` |
//!
//! A zero `f` or `t` is written as `N`. Wrappers and pairs are shared by every
//! triplet with the same parts, so they are collected as soon as nothing uses
//! them any more. A triplet's address is the address of its outer doublet,
//! which an update rewrites in place: the address stays and the old pair is
//! collected. A delete removes, like a doublet delete here, every triplet that
//! uses the deleted one as its type, from or to.
//!
//! Every doublet is classified by its sources alone:
//!
//! - `P` and `N` are the markers;
//! - `(w: P x)` is a wrapper;
//! - `(p: w x)` whose source is a wrapper is a pair;
//! - `(d: p i)` whose source is a pair is a duplicate pair;
//! - `(i: T y)` is a triplet when `y` is `i`, a pair, or a duplicate pair
//!   `(p i)` that points back at it;
//! - anything else, the `Extends` wiring of a [`TypesDecorator`](crate::TypesDecorator)
//!   say, is a doublet of somebody else and is not a triplet.
//!
//! [`Triplets`] is the interface, with every read and write as a default
//! method over [`NamedTypeLinks`], and [`TripletsOverDoublets`] is the
//! decorator that implements it over any other layer. The C# counterpart is
//! `Foundation.Data.Doublets.Cli.TripletsOverDoublets<TLinkAddress>`.
//! `docs/TRIPLETS.md` compares this encoding with the alternatives and lists
//! the cost of every operation.

mod lino;
mod permissions;
mod query;
mod restriction;

use std::collections::HashSet;

use anyhow::{bail, Result};

use crate::link::Link;
use crate::link_storage::ChangeObserver;
use crate::named_type_links::NamedTypeLinks;
use crate::types::ensure_point;

pub use lino::{import_triplets_lino, triplets_to_lino};
pub use query::{match_triplets_query, process_triplets_query, TripletChange};
pub use restriction::TripletQuery;

/// The name of the marker point of wrappers.
pub const PAIR_MARKER_NAME: &str = "Pair";

/// The name of the marker point that stands for the empty part `0`.
pub const NOTHING_MARKER_NAME: &str = "Nothing";

/// A triplet `(index: type from to)`; `from` and `to` are `0` when empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Triplet {
    pub index: u32,
    pub type_id: u32,
    pub from: u32,
    pub to: u32,
}

impl Triplet {
    pub fn new(index: u32, type_id: u32, from: u32, to: u32) -> Self {
        Self {
            index,
            type_id,
            from,
            to,
        }
    }
}

/// The two marker points of the encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TripletMarkers {
    /// `P`, the source of every wrapper `(P f)`.
    pub pair: u32,
    /// `N`, which stands for the empty part `0` inside wrappers and pairs.
    pub nothing: u32,
}

impl TripletMarkers {
    /// The points named `Pair` and `Nothing`, each created as a point
    /// `(x: x x)` and named when missing.
    pub fn ensure<L: NamedTypeLinks + ?Sized>(links: &mut L) -> Result<Self> {
        Ok(Self {
            pair: ensure_point(links, PAIR_MARKER_NAME)?,
            nothing: ensure_point(links, NOTHING_MARKER_NAME)?,
        })
    }

    /// Whether `id` is one of the markers.
    pub fn contains(&self, id: u32) -> bool {
        id == self.pair || id == self.nothing
    }

    fn encode(&self, part: u32) -> u32 {
        if part == 0 {
            self.nothing
        } else {
            part
        }
    }

    fn decode(&self, part: u32) -> u32 {
        if part == self.nothing {
            0
        } else {
            part
        }
    }
}

/// What a doublet is in the encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoubletRole {
    /// `Pair` or `Nothing`.
    Marker,
    /// `(w: P f)`, the from-part of the pairs whose source it is.
    Wrapper,
    /// `(p: w t)`, the from-part and to-part of a triplet.
    Pair,
    /// `(d: p i)`, the pair of a triplet whose outer `(T p)` another triplet took.
    DuplicatePair,
    /// The outer doublet of a triplet.
    Triplet(Triplet),
    /// A doublet the encoding does not know: not a triplet.
    Foreign,
}

impl DoubletRole {
    /// Whether the doublet is a wrapper, a pair or a duplicate pair.
    pub fn is_auxiliary(&self) -> bool {
        matches!(self, Self::Wrapper | Self::Pair | Self::DuplicatePair)
    }
}

/// The reads and writes of triplets stored in doublets.
///
/// Only [`triplet_markers`](Self::triplet_markers) is required; everything
/// else is a default method that reads and writes through `Self`, so a layer
/// that wraps a [`TripletsOverDoublets`] can implement this trait by forwarding
/// the markers and see every doublet the encoding writes. Any default can be
/// overridden: [`PermissionsDecorator`](crate::PermissionsDecorator) checks a
/// triplet as a whole, above the wrappers and pairs it is stored in.
///
/// The names end in `triplet` because a triplet store is still a doublet
/// store: `create`, `update` and `delete` of [`NamedTypeLinks`] keep working
/// on the doublets underneath.
pub trait Triplets: NamedTypeLinks {
    /// The markers of the encoding.
    fn triplet_markers(&self) -> TripletMarkers;

    /// What the doublet `id` is in the encoding; `None` when it is missing.
    fn role_of(&mut self, id: u32) -> Option<DoubletRole> {
        let markers = self.triplet_markers();
        let link = stored(self, id)?;
        Some(role(self, markers, link))
    }

    /// The triplet at `index`, `None` when `index` is not a triplet.
    fn get_triplet(&mut self, index: u32) -> Option<Triplet> {
        match self.role_of(index) {
            Some(DoubletRole::Triplet(triplet)) => Some(triplet),
            _ => None,
        }
    }

    /// Whether `index` is a triplet.
    fn is_triplet(&mut self, index: u32) -> bool {
        self.get_triplet(index).is_some()
    }

    /// Whether a reference to `index` names a triplet: the same as
    /// [`Self::is_triplet`], but for a view that hides triplets, where one the
    /// subject may refer to counts as well (issue #161).
    fn reference_is_triplet(&mut self, index: u32) -> bool {
        self.is_triplet(index)
    }

    /// Creates `(i: type from to)` and returns `i`.
    ///
    /// `type` must be a triplet; `from` and `to` must be triplets or `0`.
    fn create_triplet(&mut self, type_id: u32, from: u32, to: u32) -> Result<u32> {
        let markers = self.triplet_markers();
        ensure_type(self, type_id, None)?;
        ensure_part(self, from, None, "from")?;
        ensure_part(self, to, None, "to")?;
        if from == 0 && to == 0 {
            let index = self.try_create(0, 0)?;
            set_outer(self, index, type_id, index)?;
            return Ok(index);
        }
        let pair = pair_of(self, markers, from, to)?;
        if self.search(type_id, pair).is_none() {
            let index = self.try_create(0, 0)?;
            set_outer(self, index, type_id, pair)?;
            return Ok(index);
        }
        let index = self.try_create(0, 0)?;
        let duplicate = self.try_get_or_create(pair, index)?;
        set_outer(self, index, type_id, duplicate)?;
        Ok(index)
    }

    /// Creates a triplet `(i: i 0 0)` that is its own type: the root of a
    /// hierarchy has to start somewhere.
    fn create_triplet_point(&mut self) -> Result<u32> {
        let index = self.try_create(0, 0)?;
        self.update(index, index, index)?;
        Ok(index)
    }

    /// Creates the point `(address: address 0 0)` at an address nothing uses,
    /// the way a substitution query defines a triplet at an address it names.
    fn create_triplet_point_at(&mut self, address: u32) -> Result<()> {
        self.try_ensure_created(address)?;
        self.update(address, address, address)?;
        Ok(())
    }

    /// Makes the triplet at `index` `(index: type from to)`, keeping its
    /// address, and returns the triplet as it was.
    ///
    /// `type` must be a triplet or `index` itself; `from` and `to` must be
    /// triplets, `index` itself or `0`. The pair the triplet no longer uses is
    /// collected when nothing else uses it.
    fn update_triplet(&mut self, index: u32, type_id: u32, from: u32, to: u32) -> Result<Triplet> {
        let markers = self.triplet_markers();
        let Some(before) = self.get_triplet(index) else {
            bail!("Link {index} is not a triplet.");
        };
        ensure_type(self, type_id, Some(index))?;
        ensure_part(self, from, Some(index), "from")?;
        ensure_part(self, to, Some(index), "to")?;
        if before.type_id == type_id && before.from == from && before.to == to {
            return Ok(before);
        }
        let old_target = match stored(self, index) {
            Some(link) => link.target,
            None => bail!("Link {index} does not exist."),
        };
        let new_target = if from == 0 && to == 0 {
            index
        } else {
            let pair = pair_of(self, markers, from, to)?;
            match self.search(type_id, pair) {
                None => pair,
                Some(taken) if taken == index => pair,
                Some(_) => self.try_get_or_create(pair, index)?,
            }
        };
        set_outer(self, index, type_id, new_target)?;
        if old_target != new_target && old_target != index {
            let before = stored(self, old_target)
                .map(|link| (link, role(self, markers, link)))
                .into_iter()
                .collect();
            collect(self, markers, before)?;
        }
        Ok(before)
    }

    /// Deletes the triplet at `index` and every triplet that uses a deleted
    /// one as its type, from or to; returns them all, `index` first.
    ///
    /// Wrappers and pairs nothing uses any more are collected, so no doublet
    /// is left behind.
    fn delete_triplet(&mut self, index: u32) -> Result<Vec<Triplet>> {
        let markers = self.triplet_markers();
        let Some(first) = self.get_triplet(index) else {
            bail!("Link {index} is not a triplet.");
        };
        let deleted = cascade_of(self, first);
        // The wrappers and pairs that touch a deleted triplet, as they are now:
        // a store that deletes what uses a deleted doublet may take some of
        // them along, and their sources must still be collected.
        let mut auxiliary = Vec::new();
        for triplet in &deleted {
            let mut touching = self.query_links(None, None, Some(triplet.index));
            touching.extend(stored(self, triplet.index).and_then(|link| stored(self, link.target)));
            for link in touching {
                let doublet_role = role(self, markers, link);
                if doublet_role.is_auxiliary() {
                    auxiliary.push((link, doublet_role));
                }
            }
        }
        // Users go first, so a triplet is usually unused by the time it goes;
        // what still uses it (a cycle, a doublet of somebody else) the store
        // deletes with it, as it does with every doublet.
        for triplet in deleted.iter().rev() {
            if self.exists(triplet.index) {
                self.delete(triplet.index)?;
            }
        }
        collect(self, markers, auxiliary)?;
        Ok(deleted)
    }

    /// The triplets that match `query`, in the order of their addresses.
    ///
    /// A restriction by index is one lookup; by from (and to) it goes through
    /// the wrapper of the from-part and its pairs, by to through the pairs
    /// that end at it, by type through the doublets whose source is the type.
    /// Only a query that restricts none of them, or only an empty from or to,
    /// reads every doublet.
    fn each_triplet(&mut self, query: TripletQuery) -> Vec<Triplet> {
        let markers = self.triplet_markers();
        let mut found = match query {
            TripletQuery {
                index: Some(index), ..
            } => self.get_triplet(index).into_iter().collect(),
            TripletQuery {
                from: Some(from),
                to: Some(to),
                ..
            } if from != 0 || to != 0 => match find_pair(self, markers, from, to) {
                Some(pair) => triplets_on_pair(self, markers, pair),
                None => Vec::new(),
            },
            TripletQuery {
                from: Some(from), ..
            } if from != 0 => match self.search(markers.pair, from) {
                Some(wrapper) => {
                    let mut found = Vec::new();
                    for pair in self.query_links(None, Some(wrapper), None) {
                        found.extend(triplets_on_pair(self, markers, pair.index));
                    }
                    found
                }
                None => Vec::new(),
            },
            TripletQuery { to: Some(to), .. } if to != 0 => {
                let mut found = Vec::new();
                for pair in self.query_links(None, None, Some(to)) {
                    if is_wrapper(self, markers, pair.source) {
                        found.extend(triplets_on_pair(self, markers, pair.index));
                    }
                }
                found
            }
            TripletQuery {
                type_id: Some(type_id),
                ..
            } => {
                let mut found = Vec::new();
                if type_id != 0 && !markers.contains(type_id) {
                    for link in self.query_links(None, Some(type_id), None) {
                        if let DoubletRole::Triplet(triplet) = role(self, markers, link) {
                            found.push(triplet);
                        }
                    }
                }
                found
            }
            _ => {
                let mut found = Vec::new();
                for link in self.all_links() {
                    if let DoubletRole::Triplet(triplet) = role(self, markers, link) {
                        found.push(triplet);
                    }
                }
                found
            }
        };
        found.retain(|triplet| query.matches(triplet));
        found.sort_unstable();
        found.dedup();
        found
    }

    /// The number of triplets that match `query`.
    fn count_triplets(&mut self, query: TripletQuery) -> usize {
        self.each_triplet(query).len()
    }

    /// How many other doublets use the doublet `id` as their source or target.
    fn doublet_usages(&mut self, id: u32) -> usize {
        let as_source = self
            .query_links(None, Some(id), None)
            .into_iter()
            .filter(|link| link.index != id)
            .count();
        let as_target = self
            .query_links(None, None, Some(id))
            .into_iter()
            .filter(|link| link.index != id)
            .count();
        as_source + as_target
    }
}

fn stored<L: NamedTypeLinks + ?Sized>(links: &mut L, id: u32) -> Option<Link> {
    if id == 0 {
        return None;
    }
    links.get_link(id)
}

fn is_wrapper<L: NamedTypeLinks + ?Sized>(links: &mut L, markers: TripletMarkers, id: u32) -> bool {
    id != 0
        && !markers.contains(id)
        && stored(links, id).is_some_and(|link| link.source == markers.pair)
}

fn is_pair<L: NamedTypeLinks + ?Sized>(links: &mut L, markers: TripletMarkers, id: u32) -> bool {
    if id == 0 || markers.contains(id) {
        return false;
    }
    match stored(links, id) {
        Some(link) => is_wrapper(links, markers, link.source),
        None => false,
    }
}

/// The from-part and to-part of the pair `pair`.
fn parts_of_pair<L: NamedTypeLinks + ?Sized>(
    links: &mut L,
    markers: TripletMarkers,
    pair: Link,
) -> Option<(u32, u32)> {
    let wrapper = stored(links, pair.source)?;
    Some((markers.decode(wrapper.target), markers.decode(pair.target)))
}

/// What `link` is in the encoding; see the [module documentation](self).
fn role<L: NamedTypeLinks + ?Sized>(
    links: &mut L,
    markers: TripletMarkers,
    link: Link,
) -> DoubletRole {
    if markers.contains(link.index) {
        return DoubletRole::Marker;
    }
    if link.source == markers.pair {
        return DoubletRole::Wrapper;
    }
    if link.source == 0 || link.source == markers.nothing {
        return DoubletRole::Foreign;
    }
    let Some(source) = stored(links, link.source) else {
        return DoubletRole::Foreign;
    };
    if source.source == markers.pair {
        return DoubletRole::Pair;
    }
    if is_wrapper(links, markers, source.source) {
        return DoubletRole::DuplicatePair;
    }
    if is_pair(links, markers, source.source) {
        // The source is a duplicate pair: nothing of ours has one.
        return DoubletRole::Foreign;
    }
    if link.target == link.index {
        return DoubletRole::Triplet(Triplet::new(link.index, link.source, 0, 0));
    }
    let Some(target) = stored(links, link.target) else {
        return DoubletRole::Foreign;
    };
    if markers.contains(target.index) {
        return DoubletRole::Foreign;
    }
    if is_wrapper(links, markers, target.source) {
        if let Some((from, to)) = parts_of_pair(links, markers, target) {
            return DoubletRole::Triplet(Triplet::new(link.index, link.source, from, to));
        }
    }
    if target.target == link.index && is_pair(links, markers, target.source) {
        if let Some(pair) = stored(links, target.source) {
            if let Some((from, to)) = parts_of_pair(links, markers, pair) {
                return DoubletRole::Triplet(Triplet::new(link.index, link.source, from, to));
            }
        }
    }
    DoubletRole::Foreign
}

fn ensure_type<T: Triplets + ?Sized>(
    triplets: &mut T,
    type_id: u32,
    itself: Option<u32>,
) -> Result<()> {
    if type_id == 0 {
        bail!("The type of a triplet cannot be 0.");
    }
    if Some(type_id) == itself || triplets.is_triplet(type_id) {
        return Ok(());
    }
    bail!("The type {type_id} is not a triplet.")
}

fn ensure_part<T: Triplets + ?Sized>(
    triplets: &mut T,
    part: u32,
    itself: Option<u32>,
    name: &str,
) -> Result<()> {
    if part == 0 || Some(part) == itself || triplets.is_triplet(part) {
        return Ok(());
    }
    bail!("The {name} part {part} is not a triplet.")
}

/// The pair of `from` and `to`, with its wrapper, created when missing.
fn pair_of<L: NamedTypeLinks + ?Sized>(
    links: &mut L,
    markers: TripletMarkers,
    from: u32,
    to: u32,
) -> Result<u32> {
    let wrapper = links.try_get_or_create(markers.pair, markers.encode(from))?;
    links.try_get_or_create(wrapper, markers.encode(to))
}

/// The pair of `from` and `to`, when there is one.
fn find_pair<L: NamedTypeLinks + ?Sized>(
    links: &mut L,
    markers: TripletMarkers,
    from: u32,
    to: u32,
) -> Option<u32> {
    let wrapper = links.search(markers.pair, markers.encode(from))?;
    links.search(wrapper, markers.encode(to))
}

/// The triplets whose parts are the pair `pair`: the outers `(T pair)` and the
/// triplets of the duplicate pairs `(pair i)`.
fn triplets_on_pair<L: NamedTypeLinks + ?Sized>(
    links: &mut L,
    markers: TripletMarkers,
    pair: u32,
) -> Vec<Triplet> {
    let mut found = Vec::new();
    for outer in links.query_links(None, None, Some(pair)) {
        if let DoubletRole::Triplet(triplet) = role(links, markers, outer) {
            found.push(triplet);
        }
    }
    for duplicate in links.query_links(None, Some(pair), None) {
        if let Some(outer) = stored(links, duplicate.target) {
            if outer.target == duplicate.index {
                if let DoubletRole::Triplet(triplet) = role(links, markers, outer) {
                    found.push(triplet);
                }
            }
        }
    }
    found
}

/// The triplets other than `index` whose type, from or to is `index`.
fn users_of<T: Triplets + ?Sized>(triplets: &mut T, index: u32) -> Vec<Triplet> {
    let mut users = triplets.each_triplet(TripletQuery::by_type(index));
    users.extend(triplets.each_triplet(TripletQuery::by_from(index)));
    users.extend(triplets.each_triplet(TripletQuery::by_to(index)));
    users.retain(|user| user.index != index);
    users.sort_unstable();
    users.dedup();
    users
}

/// `first` and every triplet that uses it, directly or through another one of
/// them: what deleting `first` deletes, `first` first.
pub(crate) fn cascade_of<T: Triplets + ?Sized>(triplets: &mut T, first: Triplet) -> Vec<Triplet> {
    let mut deleted = vec![first];
    let mut seen = HashSet::from([first.index]);
    let mut at = 0;
    while at < deleted.len() {
        let used = deleted[at].index;
        for user in users_of(triplets, used) {
            if seen.insert(user.index) {
                deleted.push(user);
            }
        }
        at += 1;
    }
    deleted
}

/// Writes the outer doublet `(index: source target)`, refusing to merge it
/// into another doublet that already is `(source target)`: the store would
/// delete `index` instead.
fn set_outer<L: NamedTypeLinks + ?Sized>(
    links: &mut L,
    index: u32,
    source: u32,
    target: u32,
) -> Result<()> {
    if let Some(existing) = links
        .search(source, target)
        .filter(|&existing| existing != index)
    {
        bail!("The doublet ({source} {target}) is already {existing}: triplet {index} would be merged into it.");
    }
    links.update(index, source, target)?;
    Ok(())
}

/// Deletes each wrapper, pair or duplicate pair of `candidates` that nothing
/// uses, then what it was made of, as long as nothing else uses that either.
/// A candidate the store has already deleted leads on to what it was made of.
fn collect<T: Triplets + ?Sized>(
    triplets: &mut T,
    markers: TripletMarkers,
    candidates: Vec<(Link, DoubletRole)>,
) -> Result<()> {
    let leads_on = |doublet_role: DoubletRole| {
        matches!(doublet_role, DoubletRole::Pair | DoubletRole::DuplicatePair)
    };
    let mut pending = Vec::new();
    for (link, doublet_role) in candidates {
        if triplets.exists(link.index) {
            pending.push(link.index);
        } else if leads_on(doublet_role) {
            pending.push(link.source);
        }
    }
    while let Some(id) = pending.pop() {
        let Some(link) = stored(triplets, id) else {
            continue;
        };
        let doublet_role = role(triplets, markers, link);
        if !doublet_role.is_auxiliary() || triplets.doublet_usages(id) > 0 {
            continue;
        }
        triplets.delete(id)?;
        if leads_on(doublet_role) {
            pending.push(link.source);
        }
    }
    Ok(())
}

/// A decorator that stores triplets `(index: type from to)` in the doublets
/// of the layer it wraps; see the [module documentation](self).
pub struct TripletsOverDoublets<L: NamedTypeLinks> {
    links: L,
    markers: TripletMarkers,
}

impl<L: NamedTypeLinks> TripletsOverDoublets<L> {
    /// Triplets whose wrappers start at `pair` and whose empty part is `nothing`.
    pub fn new(links: L, pair: u32, nothing: u32) -> Self {
        Self::with_markers(links, TripletMarkers { pair, nothing })
    }

    pub fn with_markers(links: L, markers: TripletMarkers) -> Self {
        Self { links, markers }
    }

    /// Triplets whose markers are the points named `Pair` and `Nothing`,
    /// created when missing.
    pub fn with_named_markers(mut links: L) -> Result<Self> {
        let markers = TripletMarkers::ensure(&mut links)?;
        Ok(Self::with_markers(links, markers))
    }

    pub fn links(&self) -> &L {
        &self.links
    }

    pub fn links_mut(&mut self) -> &mut L {
        &mut self.links
    }

    pub fn into_inner(self) -> L {
        self.links
    }
}

impl<L: NamedTypeLinks> Triplets for TripletsOverDoublets<L> {
    fn triplet_markers(&self) -> TripletMarkers {
        self.markers
    }
}

impl<L: NamedTypeLinks> NamedTypeLinks for TripletsOverDoublets<L> {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.links.create(source, target)
    }

    fn ensure_created(&mut self, id: u32) -> u32 {
        self.links.ensure_created(id)
    }

    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        self.links.try_ensure_created(id)
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.links.get_link(id)
    }

    fn exists(&mut self, id: u32) -> bool {
        self.links.exists(id)
    }

    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        self.links.try_create(source, target)
    }

    fn try_get_or_create(&mut self, source: u32, target: u32) -> Result<u32> {
        self.links.try_get_or_create(source, target)
    }

    fn reference_exists(&mut self, id: u32) -> bool {
        self.links.reference_exists(id)
    }

    fn claims(&mut self, id: u32) -> Option<bool> {
        self.links.claims(id)
    }

    fn next_created_addresses(&mut self, count: usize) -> Result<Vec<u32>> {
        self.links.next_created_addresses(count)
    }

    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        self.links.update_observed(id, source, target, observer)
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        self.links.delete_observed(id, observer)
    }

    fn all_links(&mut self) -> Vec<Link> {
        self.links.all_links()
    }

    fn query_links(
        &mut self,
        index: Option<u32>,
        source: Option<u32>,
        target: Option<u32>,
    ) -> Vec<Link> {
        self.links.query_links(index, source, target)
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        self.links.search(source, target)
    }

    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        self.links.get_or_create(source, target)
    }

    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        self.links.get_name(id)
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        self.links.set_name(id, name)
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        self.links.get_by_name(name)
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        self.links.remove_name(id)
    }

    fn save(&mut self) -> Result<()> {
        self.links.save()
    }

    fn get_or_create_named(&mut self, name: &str) -> Result<u32> {
        self.links.get_or_create_named(name)
    }
}

/// A store lent to a layer above stays a store of triplets, as it stays
/// [`NamedTypeLinks`].
impl<T: Triplets + ?Sized> Triplets for &mut T {
    fn triplet_markers(&self) -> TripletMarkers {
        (**self).triplet_markers()
    }

    fn role_of(&mut self, id: u32) -> Option<DoubletRole> {
        (**self).role_of(id)
    }

    fn get_triplet(&mut self, index: u32) -> Option<Triplet> {
        (**self).get_triplet(index)
    }

    fn is_triplet(&mut self, index: u32) -> bool {
        (**self).is_triplet(index)
    }

    fn create_triplet(&mut self, type_id: u32, from: u32, to: u32) -> Result<u32> {
        (**self).create_triplet(type_id, from, to)
    }

    fn create_triplet_point(&mut self) -> Result<u32> {
        (**self).create_triplet_point()
    }

    fn create_triplet_point_at(&mut self, address: u32) -> Result<()> {
        (**self).create_triplet_point_at(address)
    }

    fn update_triplet(&mut self, index: u32, type_id: u32, from: u32, to: u32) -> Result<Triplet> {
        (**self).update_triplet(index, type_id, from, to)
    }

    fn delete_triplet(&mut self, index: u32) -> Result<Vec<Triplet>> {
        (**self).delete_triplet(index)
    }

    fn each_triplet(&mut self, query: TripletQuery) -> Vec<Triplet> {
        (**self).each_triplet(query)
    }
}
