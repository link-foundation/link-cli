//! Many links networks seen as one (issue #149): the joining layer of a
//! client's links with a server's, of a cache with its source, of the networks
//! of several services.
//!
//! The members are in order, and the link at an address is the one of the
//! first member that has it: a member shadows the ones after it, as a local
//! overlay of minimongo or minilinks shadows the documents of the server.
//! Reads see the links of every member that are not shadowed. Members whose
//! addresses do not overlap (a [`RemapLinks`](crate::RemapLinks) with an
//! [`OffsetAddressMap`](crate::OffsetAddressMap) moves one out of the way of
//! another) shadow nothing.
//!
//! A new link is created in the first member, the primary one. An update, a
//! delete and naming go to the member that has the link, so a link stays where
//! it lives; the store of that member resolves the write, merges and cascades
//! included, which keep to that member: a doublet is unique within its member,
//! not across them. A link created at a given address
//! ([`ensure_created`](NamedTypeLinks::ensure_created), the undo of a delete)
//! is created in the first member that [claims](NamedTypeLinks::claims) the
//! address, as a remap claims the addresses of its map, and in the primary
//! member when none does.
//!
//! A link may refer to links of other members: a link of the client typed by
//! a type of the server holds the address of the server's type. Names are
//! found in the members in order, so a name of the primary member wins.
//!
//! The union depends on no other layer. A
//! [`PermissionsDecorator`](crate::PermissionsDecorator) over it restricts
//! every member at once, its rules and types coming from any of them, and one
//! under it restricts a single member. The C# twin is
//! `UnionLinksDecorator<TLinkAddress>`, the JavaScript one `UnionLinks` of
//! `@link-foundation/link-cli-client`.

use anyhow::{bail, Result};

use crate::error::LinkError;
use crate::link::Link;
use crate::link_storage::ChangeObserver;
use crate::named_type_links::NamedTypeLinks;

/// A member of a union: any store, owned or borrowed.
pub type UnionMember<'a> = Box<dyn NamedTypeLinks + 'a>;

/// Networks seen as one; see the [module documentation](self).
pub struct UnionLinks<'a> {
    members: Vec<UnionMember<'a>>,
}

impl<'a> UnionLinks<'a> {
    /// The networks `members`, the primary one first.
    pub fn new(members: Vec<UnionMember<'a>>) -> Result<Self> {
        if members.is_empty() {
            bail!("A union needs at least one network.");
        }
        Ok(Self { members })
    }

    /// The networks, the primary one first.
    pub fn members(&self) -> &[UnionMember<'a>] {
        &self.members
    }

    /// The networks, the primary one first.
    pub fn members_mut(&mut self) -> &mut [UnionMember<'a>] {
        &mut self.members
    }

    /// The member new links are created in.
    pub fn primary(&mut self) -> &mut (dyn NamedTypeLinks + 'a) {
        &mut *self.members[0]
    }

    /// The position of the member that has the link at `index`, `None` when
    /// none has it.
    pub fn owner_of(&mut self, index: u32) -> Option<usize> {
        if index == 0 {
            return None;
        }
        self.members
            .iter_mut()
            .position(|member| member.exists(index))
    }

    /// Whether a member before the one at `member` has a link at `index`.
    fn shadowed(&mut self, member: usize, index: u32) -> bool {
        self.members[..member]
            .iter_mut()
            .any(|before| before.exists(index))
    }

    /// The member that has the link at `index`; an error when none has it.
    fn owner(&mut self, index: u32) -> Result<&mut (dyn NamedTypeLinks + 'a)> {
        match self.owner_of(index) {
            Some(owner) => Ok(&mut *self.members[owner]),
            None => Err(LinkError::not_found(index).into()),
        }
    }

    /// The member that has the link at `index`, the primary one when none has it.
    fn owner_or_primary(&mut self, index: u32) -> &mut (dyn NamedTypeLinks + 'a) {
        let owner = self.owner_of(index).unwrap_or(0);
        &mut *self.members[owner]
    }

    /// The position of the member a new link at `index` is created in: the
    /// first that claims it, the primary one when none does.
    fn place_of(&mut self, index: u32) -> usize {
        self.members
            .iter_mut()
            .position(|member| member.claims(index) == Some(true))
            .unwrap_or(0)
    }

    /// The links of every member that match, the shadowed ones left out.
    fn unshadowed(
        &mut self,
        mut read: impl FnMut(&mut dyn NamedTypeLinks) -> Vec<Link>,
    ) -> Vec<Link> {
        let mut all = Vec::new();
        for member in 0..self.members.len() {
            let links = read(&mut *self.members[member]);
            for link in links {
                if !self.shadowed(member, link.index) {
                    all.push(link);
                }
            }
        }
        all.sort_unstable_by_key(|link| link.index);
        all
    }
}

impl NamedTypeLinks for UnionLinks<'_> {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.primary().create(source, target)
    }

    fn ensure_created(&mut self, id: u32) -> u32 {
        self.try_ensure_created(id)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        if id != 0 && self.owner_of(id).is_some() {
            return Ok(id);
        }
        let place = self.place_of(id);
        self.members[place].try_ensure_created(id)
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        let owner = self.owner_of(id)?;
        self.members[owner].get_link(id)
    }

    fn exists(&mut self, id: u32) -> bool {
        self.owner_of(id).is_some()
    }

    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        self.primary().try_create(source, target)
    }

    fn reference_exists(&mut self, id: u32) -> bool {
        self.members
            .iter_mut()
            .any(|member| member.reference_exists(id))
    }

    /// A plain member has every address; a composed one those it claims.
    fn claims(&mut self, id: u32) -> Option<bool> {
        Some(
            self.members
                .iter_mut()
                .any(|member| member.claims(id) != Some(false)),
        )
    }

    fn next_created_addresses(&mut self, count: usize) -> Result<Vec<u32>> {
        self.primary().next_created_addresses(count)
    }

    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        self.owner(id)?
            .update_observed(id, source, target, observer)
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        self.owner(id)?.delete_observed(id, observer)
    }

    fn all_links(&mut self) -> Vec<Link> {
        self.unshadowed(|member| member.all_links())
    }

    fn query_links(
        &mut self,
        index: Option<u32>,
        source: Option<u32>,
        target: Option<u32>,
    ) -> Vec<Link> {
        self.unshadowed(|member| member.query_links(index, source, target))
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        for member in 0..self.members.len() {
            if let Some(found) = self.members[member].search(source, target) {
                if !self.shadowed(member, found) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        match self.search(source, target) {
            Some(found) => found,
            None => self.create(source, target),
        }
    }

    /// The name of `id` in the member that has it, else in the first member
    /// that names it.
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        if let Some(owner) = self.owner_of(id) {
            return self.members[owner].get_name(id);
        }
        for member in self.members.iter_mut() {
            if let Some(name) = member.get_name(id)? {
                return Ok(Some(name));
            }
        }
        Ok(None)
    }

    /// Names `id` in the member that has it, the primary one when none does.
    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        self.owner_or_primary(id).set_name(id, name)
    }

    /// The link of `name` in the first member that has a link of that name
    /// it does not shadow.
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        for member in 0..self.members.len() {
            if let Some(found) = self.members[member].get_by_name(name)? {
                if found != 0 && !self.shadowed(member, found) {
                    return Ok(Some(found));
                }
            }
        }
        Ok(None)
    }

    /// Unnames `id` in the member that has it, the primary one when none does.
    fn remove_name(&mut self, id: u32) -> Result<()> {
        self.owner_or_primary(id).remove_name(id)
    }

    fn save(&mut self) -> Result<()> {
        for member in self.members.iter_mut() {
            member.save()?;
        }
        Ok(())
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
