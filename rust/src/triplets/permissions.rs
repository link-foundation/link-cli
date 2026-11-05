//! Permissions on triplets (issues #149 and #152): the same rules as on
//! doublets, judged on the triplet as a whole.
//!
//! The doublets of a triplet are not the subject's business. A rule on the type
//! `Note` says nothing about the wrappers and pairs that happen to store the
//! parts of a note, so a [`PermissionsDecorator`] under
//! [`TripletsOverDoublets`](super::TripletsOverDoublets) would hide them and
//! refuse to write them. It goes above instead, over the [`Triplets`] store,
//! and asks the policy only about the outer doublet `(index: type …)`:
//!
//! - a triplet the subject may not read does not exist for the subject, and
//!   neither does anything that is not a triplet;
//! - creating needs the right to create and to write the type;
//! - every part needs the right to refer to it (issue #161): without it an
//!   unreadable part answers as a missing one, after the write check, and a
//!   part the triplet already holds may stay;
//! - choosing the address of a new point is a reference too;
//! - updating needs the right to write the triplet as it is and as it will be;
//! - deleting needs the right to write every triplet the delete takes along,
//!   or nothing is deleted;
//! - a point the subject created, still only its own type, is the subject's:
//!   that is what a substitution query reserves before it sets the type.

use anyhow::{bail, Result};

use super::{cascade_of, DoubletRole, Triplet, TripletMarkers, TripletQuery, Triplets};
use crate::error::LinkError;
use crate::link::Link;
use crate::permissions::{AccessPolicy, Forbidden, PermissionsDecorator};

impl<L: Triplets, P: AccessPolicy> PermissionsDecorator<L, P> {
    /// The outer doublet of a triplet the store below holds.
    fn outer_of(&mut self, index: u32) -> Option<Link> {
        self.inner_mut().get_link(index)
    }

    fn triplet_readable(&mut self, triplet: &Triplet) -> bool {
        self.outer_of(triplet.index)
            .is_some_and(|outer| self.can_read_triplet(&outer))
    }

    fn triplet_writable(&mut self, triplet: &Triplet) -> bool {
        self.outer_of(triplet.index)
            .is_some_and(|outer| self.can_write_triplet(&outer))
    }

    /// `from` or `to` must be `0`, the triplet itself, a part `before` holds
    /// or a triplet the subject may refer to; a hidden one answers as a missing one.
    fn check_part(&mut self, part: u32, before: Option<&Triplet>, name: &str) -> Result<()> {
        if part == 0 || before.is_some_and(|before| holds(before, part)) {
            return Ok(());
        }
        if self.reference_is_triplet(part) {
            return Ok(());
        }
        bail!("The {name} part {part} is not a triplet.")
    }
}

/// Whether `part` is `triplet` or one of its parts.
fn holds(triplet: &Triplet, part: u32) -> bool {
    part == triplet.index || part == triplet.type_id || part == triplet.from || part == triplet.to
}

impl<L: Triplets, P: AccessPolicy> Triplets for PermissionsDecorator<L, P> {
    fn triplet_markers(&self) -> TripletMarkers {
        self.inner().triplet_markers()
    }

    fn role_of(&mut self, id: u32) -> Option<DoubletRole> {
        let link = self.outer_of(id)?;
        if !self.can_read_triplet(&link) {
            return None;
        }
        self.inner_mut().role_of(id)
    }

    fn reference_is_triplet(&mut self, index: u32) -> bool {
        self.is_triplet(index) || (self.can_reference(index) && self.inner_mut().is_triplet(index))
    }

    fn create_triplet(&mut self, type_id: u32, from: u32, to: u32) -> Result<u32> {
        if !self.policy_can_create()
            || (type_id != 0 && !self.policy_can_write(&Link::new(0, type_id, 0)))
        {
            return Err(Forbidden::new("create").into());
        }
        // A subject that may write a type need not read it: the rule that says
        // so names it. That it exists is left to the layer below.
        if type_id != 0 && !self.can_reference(type_id) {
            return Err(LinkError::not_found(type_id).into());
        }
        self.check_part(from, None, "from")?;
        self.check_part(to, None, "to")?;
        let index = self.inner_mut().create_triplet(type_id, from, to)?;
        self.remember(index);
        Ok(index)
    }

    fn create_triplet_point(&mut self) -> Result<u32> {
        if !self.policy_can_create() {
            return Err(Forbidden::new("create").into());
        }
        let index = self.inner_mut().create_triplet_point()?;
        self.remember(index);
        Ok(index)
    }

    fn create_triplet_point_at(&mut self, address: u32) -> Result<()> {
        if !self.policy_can_create() {
            return Err(Forbidden::new("create").into());
        }
        // Choosing an address is a reference: without the right a hidden link
        // and a free address answer alike.
        if !self.can_reference(address) {
            return Err(LinkError::not_found(address).into());
        }
        // The subject may refer to this hidden link: the point must not land on it.
        if self.inner_mut().exists(address) {
            bail!("Link {address} exists");
        }
        self.inner_mut().create_triplet_point_at(address)?;
        self.remember(address);
        Ok(())
    }

    fn update_triplet(&mut self, index: u32, type_id: u32, from: u32, to: u32) -> Result<Triplet> {
        let Some(before) = self.get_triplet(index) else {
            bail!("Link {index} is not a triplet.");
        };
        // Only an empty triplet has itself as its target.
        let target = if from == 0 && to == 0 { index } else { 0 };
        if !self.triplet_writable(&before)
            || !self.can_write_triplet(&Link::new(index, type_id, target))
        {
            return Err(Forbidden::new(format!("update {index}")).into());
        }
        if type_id != 0 && !holds(&before, type_id) && !self.can_reference(type_id) {
            return Err(LinkError::not_found(type_id).into());
        }
        self.check_part(from, Some(&before), "from")?;
        self.check_part(to, Some(&before), "to")?;
        self.inner_mut().update_triplet(index, type_id, from, to)
    }

    fn delete_triplet(&mut self, index: u32) -> Result<Vec<Triplet>> {
        let Some(first) = self.get_triplet(index) else {
            bail!("Link {index} is not a triplet.");
        };
        for triplet in cascade_of(self.inner_mut(), first) {
            if !self.triplet_writable(&triplet) {
                return Err(Forbidden::new(format!("delete {index}")).into());
            }
        }
        let deleted = self.inner_mut().delete_triplet(index)?;
        for triplet in &deleted {
            self.forget(triplet.index);
        }
        Ok(deleted)
    }

    fn each_triplet(&mut self, query: TripletQuery) -> Vec<Triplet> {
        let mut found = self.inner_mut().each_triplet(query);
        found.retain(|triplet| self.triplet_readable(triplet));
        found
    }
}
