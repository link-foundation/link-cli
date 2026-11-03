//! A links network under other addresses (issue #149): every address that goes
//! down is mapped to the network's own, and every address that comes up is
//! mapped back, so the code above never thinks about remapping.
//!
//! The map is an [`AddressMap`]: an [`OffsetAddressMap`] moves a network out
//! of the way of another, so a [`UnionLinks`](crate::UnionLinks) of them
//! shadows nothing, and an [`AddressTable`] gives the addresses of another
//! system addresses of our own. Strings from outside, such as UUIDs, are bound
//! to links by [`LinksExternalIds`](crate::LinksExternalIds).
//!
//! A read that names an address that is not of the network below finds
//! nothing; a write that names one fails. The null link reads the same on
//! both sides. The C# twin is `RemapLinksDecorator<TLinkAddress>`, the
//! JavaScript one `RemapLinks` of `@link-foundation/link-cli-client`, whose
//! addresses above may be strings such as UUIDs.

use std::collections::BTreeMap;

use anyhow::{bail, Result};

use crate::error::LinkError;
use crate::link::Link;
use crate::link_storage::ChangeObserver;
use crate::named_type_links::NamedTypeLinks;

/// How the addresses of a network below a [`RemapLinks`] read above it. Only
/// addresses of links are mapped: the null link is the null link on both
/// sides.
pub trait AddressMap {
    /// The address below of `outer`; `None` when it is no address of the
    /// network below.
    fn to_inner(&self, outer: u32) -> Option<u32>;

    /// The address above of `inner`, given one when it has none yet.
    fn to_outer(&mut self, inner: u32) -> Result<u32>;
}

/// Every address moved by the same offset: the network below reads at
/// `offset + 1` and up, so networks moved by different offsets share no
/// address and a [`UnionLinks`](crate::UnionLinks) of them shadows nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffsetAddressMap {
    /// What every address of the network below is moved by.
    pub offset: u32,
}

impl OffsetAddressMap {
    pub fn new(offset: u32) -> Self {
        Self { offset }
    }
}

impl AddressMap for OffsetAddressMap {
    fn to_inner(&self, outer: u32) -> Option<u32> {
        (outer > self.offset).then(|| outer - self.offset)
    }

    fn to_outer(&mut self, inner: u32) -> Result<u32> {
        match inner.checked_add(self.offset) {
            Some(outer) if outer != u32::MAX => Ok(outer),
            _ => bail!(
                "Address {inner} moved by {} is past the last address.",
                self.offset
            ),
        }
    }
}

/// A table of the addresses above and below: an address below that has none
/// above yet gets the next free one from `first` on, so the addresses of
/// another system (a server, a service) read as addresses of our own range.
/// [`pairs`](Self::pairs) and [`add`](Self::add) keep the table across runs,
/// in links or anywhere else.
#[derive(Debug, Clone, Default)]
pub struct AddressTable {
    inner: BTreeMap<u32, u32>,
    outer: BTreeMap<u32, u32>,
    next: u32,
}

impl AddressTable {
    /// A table that gives addresses from `first` on.
    pub fn new(first: u32) -> Result<Self> {
        if first == 0 {
            bail!("The null link is no address to give.");
        }
        Ok(Self {
            inner: BTreeMap::new(),
            outer: BTreeMap::new(),
            next: first,
        })
    }

    /// Every address above with its address below, by the address above.
    pub fn pairs(&self) -> Vec<(u32, u32)> {
        self.inner
            .iter()
            .map(|(&outer, &inner)| (outer, inner))
            .collect()
    }

    /// Maps `outer` to `inner`; neither may be mapped to another address
    /// already.
    pub fn add(&mut self, outer: u32, inner: u32) -> Result<()> {
        if outer == 0 || inner == 0 {
            bail!("The null link is mapped to itself.");
        }
        if let Some(&known) = self.inner.get(&outer).filter(|&&known| known != inner) {
            bail!("Address {outer} is mapped to {known} already.");
        }
        if let Some(&known) = self.outer.get(&inner).filter(|&&known| known != outer) {
            bail!("Address {inner} is mapped from {known} already.");
        }
        self.inner.insert(outer, inner);
        self.outer.insert(inner, outer);
        Ok(())
    }
}

impl AddressMap for AddressTable {
    fn to_inner(&self, outer: u32) -> Option<u32> {
        self.inner.get(&outer).copied()
    }

    fn to_outer(&mut self, inner: u32) -> Result<u32> {
        if let Some(&outer) = self.outer.get(&inner) {
            return Ok(outer);
        }
        while self.inner.contains_key(&self.next) {
            self.next += 1;
        }
        let outer = self.next;
        self.next += 1;
        self.add(outer, inner)?;
        Ok(outer)
    }
}

/// `links` under the addresses of `map`; see the [module documentation](self).
pub struct RemapLinks<L: NamedTypeLinks, M: AddressMap> {
    links: L,
    map: M,
}

impl<L: NamedTypeLinks, M: AddressMap> RemapLinks<L, M> {
    /// `links` whose addresses read as `map` tells.
    pub fn new(links: L, map: M) -> Self {
        Self { links, map }
    }

    /// How the addresses of the network below read above.
    pub fn map(&self) -> &M {
        &self.map
    }

    pub fn inner(&self) -> &L {
        &self.links
    }

    pub fn inner_mut(&mut self) -> &mut L {
        &mut self.links
    }

    pub fn into_inner(self) -> (L, M) {
        (self.links, self.map)
    }

    /// The address above of `inner`; the null link reads the same.
    pub fn to_outer(&mut self, inner: u32) -> Result<u32> {
        if inner == 0 {
            return Ok(0);
        }
        self.map.to_outer(inner)
    }

    /// The address below of `outer`; `None` when it is no address of the
    /// network below. The null link reads the same.
    pub fn to_inner(&self, outer: u32) -> Option<u32> {
        if outer == 0 {
            return Some(0);
        }
        self.map.to_inner(outer)
    }

    /// The address below of a link `outer` the network below should have.
    fn found(&self, outer: u32) -> Result<u32> {
        match self.to_inner(outer) {
            Some(inner) if outer != 0 => Ok(inner),
            _ => Err(LinkError::not_found(outer).into()),
        }
    }

    /// The address below of an address a link of the network below refers to.
    fn referred(&self, outer: u32) -> Result<u32> {
        self.to_inner(outer).ok_or_else(|| {
            anyhow::anyhow!(
                "A link of this network cannot refer to {outer}: the address is not of this network."
            )
        })
    }

    fn outer_link(&mut self, link: Link) -> Result<Link> {
        Ok(Link::new(
            self.to_outer(link.index)?,
            self.to_outer(link.source)?,
            self.to_outer(link.target)?,
        ))
    }

    fn outer_or_panic(&mut self, inner: u32) -> u32 {
        self.to_outer(inner)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn outer_links(&mut self, links: Vec<Link>) -> Vec<Link> {
        links
            .into_iter()
            .map(|link| {
                self.outer_link(link)
                    .unwrap_or_else(|error| panic!("{error}"))
            })
            .collect()
    }

    /// Calls `write` on the store below with an observer that reports the
    /// changes under the addresses above.
    fn observed(
        &mut self,
        observer: ChangeObserver<'_>,
        write: impl FnOnce(&mut L, ChangeObserver<'_>) -> Result<Link>,
    ) -> Result<Link> {
        let mut changes = Vec::new();
        let result = write(&mut self.links, &mut |before, after| {
            changes.push((before, after))
        });
        for (before, after) in changes {
            let before = self.outer_link(before)?;
            let after = self.outer_link(after)?;
            observer(before, after);
        }
        self.outer_link(result?)
    }
}

impl<L: NamedTypeLinks, M: AddressMap> NamedTypeLinks for RemapLinks<L, M> {
    /// Panics when `source` or `target` is not of this network; use
    /// [`try_create`](NamedTypeLinks::try_create).
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.try_create(source, target)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Panics when `id` is not of this network; use
    /// [`try_ensure_created`](NamedTypeLinks::try_ensure_created).
    fn ensure_created(&mut self, id: u32) -> u32 {
        self.try_ensure_created(id)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        let inner = self.referred(id)?;
        let created = self.links.try_ensure_created(inner)?;
        self.to_outer(created)
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        let inner = self.to_inner(id).filter(|&inner| inner != 0)?;
        let link = self.links.get_link(inner)?;
        self.outer_link(link).ok()
    }

    fn exists(&mut self, id: u32) -> bool {
        self.to_inner(id)
            .is_some_and(|inner| inner != 0 && self.links.exists(inner))
    }

    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        let (source, target) = (self.referred(source)?, self.referred(target)?);
        let created = self.links.try_create(source, target)?;
        self.to_outer(created)
    }

    fn reference_exists(&mut self, id: u32) -> bool {
        self.to_inner(id)
            .is_some_and(|inner| inner != 0 && self.links.reference_exists(inner))
    }

    fn claims(&mut self, id: u32) -> Option<bool> {
        Some(self.to_inner(id).is_some())
    }

    fn next_created_addresses(&mut self, count: usize) -> Result<Vec<u32>> {
        let addresses = self.links.next_created_addresses(count)?;
        addresses
            .into_iter()
            .map(|address| self.to_outer(address))
            .collect()
    }

    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        let id = self.found(id)?;
        let (source, target) = (self.referred(source)?, self.referred(target)?);
        self.observed(observer, |links, observer| {
            links.update_observed(id, source, target, observer)
        })
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        let id = self.found(id)?;
        self.observed(observer, |links, observer| {
            links.delete_observed(id, observer)
        })
    }

    fn all_links(&mut self) -> Vec<Link> {
        let links = self.links.all_links();
        self.outer_links(links)
    }

    fn query_links(
        &mut self,
        index: Option<u32>,
        source: Option<u32>,
        target: Option<u32>,
    ) -> Vec<Link> {
        let mut inner = [None; 3];
        for (part, outer) in [index, source, target].into_iter().enumerate() {
            if let Some(outer) = outer {
                match self.to_inner(outer) {
                    Some(address) => inner[part] = Some(address),
                    // No link of the network below refers to it.
                    None => return Vec::new(),
                }
            }
        }
        let links = self.links.query_links(inner[0], inner[1], inner[2]);
        let mut links = self.outer_links(links);
        links.sort_unstable_by_key(|link| link.index);
        links
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        let (source, target) = (self.to_inner(source)?, self.to_inner(target)?);
        let found = self.links.search(source, target)?;
        Some(self.outer_or_panic(found))
    }

    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        match self.search(source, target) {
            Some(found) => found,
            None => self.create(source, target),
        }
    }

    fn try_get_or_create(&mut self, source: u32, target: u32) -> Result<u32> {
        match self.search(source, target) {
            Some(found) => Ok(found),
            None => self.try_create(source, target),
        }
    }

    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        match self.to_inner(id) {
            Some(inner) if inner != 0 => self.links.get_name(inner),
            _ => Ok(None),
        }
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        let inner = self.found(id)?;
        self.links.set_name(inner, name)?;
        Ok(id)
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        match self.links.get_by_name(name)? {
            Some(inner) => Ok(Some(self.to_outer(inner)?)),
            None => Ok(None),
        }
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        match self.to_inner(id) {
            Some(inner) if inner != 0 => self.links.remove_name(inner),
            _ => Ok(()),
        }
    }

    fn save(&mut self) -> Result<()> {
        self.links.save()
    }

    fn get_or_create_named(&mut self, name: &str) -> Result<u32> {
        if let Some(id) = self.get_by_name(name)? {
            return Ok(id);
        }
        let id = self.try_create(0, 0)?;
        self.set_name(id, name)?;
        self.update(id, id, id)?;
        Ok(id)
    }
}
