//! A type hierarchy with recursive sub-typing and multiple inheritance, kept
//! in the links themselves (issue #148).
//!
//! - The type of a link is its source: `(v: S x)` is a value of type `S`.
//! - A type points at itself: the root `(Type: Type Type)` is a point, and a
//!   sub-type of `T` is `(S: T S)`, a value of `T` whose target is itself.
//! - More parents (multiple inheritance) are pairs `(p: S T2)` marked by
//!   `(m: Extends p)`.
//!
//! So `Type -> SubType -> … -> Value` is a chain of sources of any depth, and
//! every query below walks it with a set of visited links, so cycles end.
//!
//! [`Types`] is the interface, with every query and write as a default method
//! over [`NamedTypeLinks`], and [`TypesDecorator`] is the decorator that
//! implements it over any other layer. It is the Rust port of `typeSystem`
//! of `@link-foundation/ui-links`; the C# counterpart is
//! `Foundation.Data.Doublets.Cli.TypesDecorator<TLinkAddress>`.
//!
//! The pinned types of [`PinnedTypes`](crate::pinned_types::PinnedTypes) are
//! `(1: 1 1)`, `(2: 1 2)`, `(3: 1 3)`, …: with the root at address 1 they are
//! sub-types of it, so a pinned store is already a hierarchy.

use std::collections::HashSet;

use anyhow::{bail, Result};

use crate::link::Link;
use crate::link_storage::ChangeObserver;
use crate::named_type_links::NamedTypeLinks;

/// The name of the root type point.
pub const ROOT_TYPE_NAME: &str = "Type";

/// The name of the `Extends` marker point.
pub const EXTENDS_NAME: &str = "Extends";

/// The two points a type hierarchy needs: the root type `(Type: Type Type)`
/// and the `Extends` marker of extra parents, `None` when the hierarchy has
/// no multiple inheritance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeMarkers {
    pub root: u32,
    pub extends: Option<u32>,
}

impl TypeMarkers {
    /// The points named `Type` and `Extends`, each created as a point
    /// `(x: x x)` and named when missing.
    pub fn ensure<L: NamedTypeLinks + ?Sized>(links: &mut L) -> Result<Self> {
        Ok(Self {
            root: ensure_point(links, ROOT_TYPE_NAME)?,
            extends: Some(ensure_point(links, EXTENDS_NAME)?),
        })
    }
}

/// The point named `name`, created and named when missing.
///
/// A name whose link no longer exists is given to a new point.
pub fn ensure_point<L: NamedTypeLinks + ?Sized>(links: &mut L, name: &str) -> Result<u32> {
    if let Some(known) = links.get_by_name(name)? {
        if known != 0 && links.exists(known) {
            return Ok(known);
        }
    }
    let id = links.try_create(0, 0)?;
    links.update(id, id, id)?;
    links.set_name(id, name)?;
    Ok(id)
}

/// The queries and writes of a type hierarchy.
///
/// Only [`root`](Self::root) and [`extends`](Self::extends) are required;
/// everything else is a default method that reads and writes through `Self`,
/// so a layer that wraps a [`TypesDecorator`] -- a permission check, say --
/// implements this trait by forwarding the two markers and sees every write
/// the hierarchy makes. Any default can be overridden.
pub trait Types: NamedTypeLinks {
    /// The root type, `(Type: Type Type)`.
    fn root(&self) -> u32;

    /// The `Extends` marker, or `None` when there is none.
    fn extends(&self) -> Option<u32>;

    /// The type of a link: its source; `None` for a missing link.
    fn type_of(&mut self, link: u32) -> Option<u32> {
        stored(self, link).map(|link| link.source)
    }

    /// Whether `link` is a type: it exists and points at itself.
    fn is_type(&mut self, link: u32) -> bool {
        stored(self, link).is_some_and(|stored| stored.target == link)
    }

    /// The direct parents of a type: its source (unless it is a point) and
    /// what it extends, in the order the extensions were added.
    fn parents_of(&mut self, type_id: u32) -> Vec<u32> {
        let mut parents = Vec::new();
        let Some(link) = stored(self, type_id) else {
            return parents;
        };
        if link.source != type_id && link.source != 0 {
            parents.push(link.source);
        }
        add_extended_types(self, type_id, &mut parents);
        parents
    }

    /// Every type `type_id` is, itself first, breadth first, without repeats.
    fn ancestors_of(&mut self, type_id: u32) -> Vec<u32> {
        let mut ancestors = vec![type_id];
        let mut seen = HashSet::from([type_id]);
        let mut i = 0;
        while i < ancestors.len() {
            for parent in self.parents_of(ancestors[i]) {
                if seen.insert(parent) {
                    ancestors.push(parent);
                }
            }
            i += 1;
        }
        ancestors
    }

    /// Whether `link` is a pair that gives a type one more parent.
    fn is_extension(&mut self, link: u32) -> bool {
        match self.extends() {
            Some(extends) if link != 0 => self.search(extends, link).is_some(),
            _ => false,
        }
    }

    /// Whether `link` is strictly of `type_id`: starting from its source and
    /// the types the link itself extends, through any depth of sub-types and
    /// parents, cycles included. A type is not of itself; [`is_a`](Self::is_a) is.
    fn is_of(&mut self, link: u32, type_id: u32) -> bool {
        match stored(self, link) {
            Some(link) => self.is_of_link(&link, type_id),
            None => false,
        }
    }

    /// [`is_of`](Self::is_of) for a link given as `(index, source, target)`,
    /// stored or not.
    fn is_of_link(&mut self, link: &Link, type_id: u32) -> bool {
        // A link is of its source and of every type it extends itself; the
        // pairs that give a type more parents are of type Extends, not values
        // of the type they extend.
        let mut starts = Vec::new();
        match self.extends() {
            Some(extends) if self.is_extension(link.index) => starts.push(extends),
            _ => {
                starts.push(link.source);
                if link.index != 0 && self.exists(link.index) {
                    add_extended_types(self, link.index, &mut starts);
                }
            }
        }
        let mut stack: Vec<u32> = starts.into_iter().rev().collect();
        let mut seen = HashSet::new();
        while let Some(at) = stack.pop() {
            if at == type_id {
                return true;
            }
            if at == 0 || !seen.insert(at) {
                continue;
            }
            stack.extend(self.parents_of(at));
        }
        false
    }

    /// Whether `link` is `type_id` itself or of it.
    fn is_a(&mut self, link: u32, type_id: u32) -> bool {
        stored(self, link).is_some() && (link == type_id || self.is_of(link, type_id))
    }

    /// [`is_a`](Self::is_a) for a link given as `(index, source, target)`,
    /// stored or not.
    fn is_a_link(&mut self, link: &Link, type_id: u32) -> bool {
        link.index == type_id || self.is_of_link(link, type_id)
    }

    /// Defines a type `(S: parent S)` that also extends every type in `more`.
    fn define(&mut self, parent: u32, more: &[u32]) -> Result<u32> {
        if !more.is_empty() && self.extends().is_none() {
            bail!("Multiple parents need an Extends marker.");
        }
        ensure_exists(self, parent)?;
        for &other in more {
            ensure_exists(self, other)?;
        }
        let id = self.try_create(0, 0)?;
        self.update(id, parent, id)?;
        for &other in more {
            self.extend(id, other)?;
        }
        Ok(id)
    }

    /// Defines a direct sub-type of [`root`](Self::root).
    fn define_root(&mut self) -> Result<u32> {
        let root = self.root();
        self.define(root, &[])
    }

    /// Makes `type_id` extend `parent`: the pair `(p: type parent)` marked by
    /// `(m: Extends p)`. Returns the pair.
    ///
    /// Refused when `parent` is a direct sub-type `(parent: type parent)` of
    /// `type_id`, whose definition is that very doublet.
    fn extend(&mut self, type_id: u32, parent: u32) -> Result<u32> {
        let Some(extends) = self.extends() else {
            bail!("Multiple parents need an Extends marker.");
        };
        ensure_exists(self, type_id)?;
        let parent_link = ensure_exists(self, parent)?;
        if parent_link.source == type_id && parent_link.target == parent {
            // (parent: type parent) is already the doublet (type parent):
            // marking it would turn the definition of parent into a pair.
            bail!(
                "Type {parent} is a direct sub-type of {type_id}: the pair ({type_id} {parent}) would be its definition."
            );
        }
        let pair = self.try_get_or_create(type_id, parent)?;
        self.try_get_or_create(extends, pair)?;
        Ok(pair)
    }

    /// A value `(v: type v)` of `type_id`.
    fn instance(&mut self, type_id: u32) -> Result<u32> {
        ensure_exists(self, type_id)?;
        let id = self.try_create(0, 0)?;
        self.update(id, type_id, id)?;
        Ok(id)
    }

    /// A value `(v: type target)` of `type_id`.
    fn instance_with_target(&mut self, type_id: u32, target: u32) -> Result<u32> {
        ensure_exists(self, type_id)?;
        ensure_exists(self, target)?;
        self.try_get_or_create(type_id, target)
    }
}

fn stored<L: NamedTypeLinks + ?Sized>(links: &mut L, link: u32) -> Option<Link> {
    if link == 0 {
        return None;
    }
    links.get_link(link)
}

fn ensure_exists<L: NamedTypeLinks + ?Sized>(links: &mut L, link: u32) -> Result<Link> {
    match stored(links, link) {
        Some(stored) => Ok(stored),
        None => bail!("Link {link} does not exist."),
    }
}

/// Adds the targets of the pairs `(p: type T)` marked by `(m: Extends p)`, in
/// the order of the pairs' addresses.
fn add_extended_types<T: Types + ?Sized>(types: &mut T, type_id: u32, parents: &mut Vec<u32>) {
    if types.extends().is_none() || type_id == 0 {
        return;
    }
    for pair in types.query_links(None, Some(type_id), None) {
        if pair.index != type_id && pair.target != 0 && types.is_extension(pair.index) {
            parents.push(pair.target);
        }
    }
}

/// A decorator that keeps a type hierarchy with recursive sub-typing and
/// multiple inheritance in the links of the layer it wraps; see the
/// [module documentation](self).
pub struct TypesDecorator<L: NamedTypeLinks> {
    links: L,
    markers: TypeMarkers,
}

impl<L: NamedTypeLinks> TypesDecorator<L> {
    /// A hierarchy rooted at `root`, with `extends` as the marker of extra
    /// parents, or without multiple inheritance when it is `None`.
    pub fn new(links: L, root: u32, extends: Option<u32>) -> Self {
        Self::with_markers(links, TypeMarkers { root, extends })
    }

    pub fn with_markers(links: L, markers: TypeMarkers) -> Self {
        Self { links, markers }
    }

    /// A hierarchy whose markers are the points named `Type` and `Extends`,
    /// created when missing.
    pub fn with_named_markers(mut links: L) -> Result<Self> {
        let markers = TypeMarkers::ensure(&mut links)?;
        Ok(Self::with_markers(links, markers))
    }

    pub fn markers(&self) -> TypeMarkers {
        self.markers
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

impl<L: NamedTypeLinks> Types for TypesDecorator<L> {
    fn root(&self) -> u32 {
        self.markers.root
    }

    fn extends(&self) -> Option<u32> {
        self.markers.extends
    }
}

impl<L: NamedTypeLinks> NamedTypeLinks for TypesDecorator<L> {
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
