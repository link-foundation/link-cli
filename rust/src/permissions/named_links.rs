//! The [`NamedTypeLinks`] implementation of [`PermissionsDecorator`]: every
//! read, write and name operation checked against the decorator's policy.
//!
//! Extracted from `permissions.rs` for issue #172 to keep that file below the
//! 900-line warning threshold of `rust/scripts/check-file-size.rs`.

use anyhow::{bail, Result};

use crate::error::LinkError;
use crate::link::Link;
use crate::link_storage::ChangeObserver;
use crate::named_type_links::NamedTypeLinks;
use crate::undoable_links::{keep_names_before_update, LinksUndoLog};

use super::{AccessPolicy, Forbidden, PermissionsDecorator};

impl<L: NamedTypeLinks, P: AccessPolicy> NamedTypeLinks for PermissionsDecorator<L, P> {
    fn query_limits(&self) -> Option<crate::query_options::QueryLimits> {
        self.links.query_limits()
    }

    fn reserve_query_allocations(
        &mut self,
        processor: &crate::query_processor::QueryProcessor,
        restriction: &crate::lino_link::LinoLink,
        substitution: &crate::lino_link::LinoLink,
    ) -> Result<Option<std::collections::HashSet<u32>>> {
        let reserved =
            self.links
                .reserve_query_allocations(processor, restriction, substitution)?;
        // A preview lists every row of its scratch store. Only addresses that do
        // not exist yet are allocations; counting an existing row would let a
        // hidden address pass as one and answer differently from a missing one.
        Ok(reserved.map(|mut ids| {
            ids.retain(|&id| !self.links.reference_exists(id));
            ids
        }))
    }

    /// Panics when the subject may not create; use
    /// [`try_create`](NamedTypeLinks::try_create).
    fn create(&mut self, source: u32, target: u32) -> u32 {
        Self::panic_on(self.try_create(source, target))
    }

    /// Panics when the subject may not create; use
    /// [`try_ensure_created`](NamedTypeLinks::try_ensure_created).
    fn ensure_created(&mut self, id: u32) -> u32 {
        Self::panic_on(self.try_ensure_created(id))
    }

    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        if id != 0 && self.exists(id) {
            return Ok(id);
        }
        if !self.policy_can_create() {
            return Err(Forbidden::new("create").into());
        }
        if id != 0 && !self.can_reference(id) {
            return Err(LinkError::not_found(id).into());
        }
        if id != 0 && self.links.exists(id) {
            // The subject may refer to this hidden link: the point must not land on it.
            bail!("Link {id} exists");
        }
        let created = self.links.try_ensure_created(id)?;
        if id != 0 {
            self.created.insert(created);
        }
        Ok(created)
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        let link = self.links.get_link(id)?;
        self.can_read(&link).then_some(link)
    }

    fn exists(&mut self, id: u32) -> bool {
        self.get_link(id).is_some()
    }

    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        self.require_create(source, target)?;
        self.check_references(None, source, target)?;
        let id = self.links.try_create(source, target)?;
        self.created.insert(id);
        Ok(id)
    }

    /// A link the subject may not refer to is as missing as a deleted one.
    fn reference_exists(&mut self, id: u32) -> bool {
        self.links.reference_exists(id) && self.can_reference(id)
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
        let Some(before) = self.visible(id)? else {
            return self.links.update_observed(id, source, target, observer);
        };
        let after = Link::new(id, source, target);
        if !(self.owns(&before) && source == 0)
            && (!self.can_write(&before) || !self.policy_can_write(&after))
        {
            return Err(Forbidden::new(format!("update {id}")).into());
        }
        self.check_references(Some(&before), source, target)?;
        let mut log = LinksUndoLog::new();
        keep_names_before_update(&mut log, &mut self.links, id, source, target)?;
        self.checked(
            &format!("update {id}"),
            id,
            log,
            |links, recorder| links.update_observed(id, source, target, recorder),
            observer,
        )
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        let Some(before) = self.visible(id)? else {
            return self.links.delete_observed(id, observer);
        };
        if !self.can_write(&before) {
            return Err(Forbidden::new(format!("delete {id}")).into());
        }
        let mut log = LinksUndoLog::new();
        log.keep_names_of_usages(&mut self.links, id)?;
        self.checked(
            &format!("delete {id}"),
            id,
            log,
            |links, recorder| links.delete_observed(id, recorder),
            observer,
        )
    }

    fn all_links(&mut self) -> Vec<Link> {
        let links = self.links.all_links();
        links
            .into_iter()
            .filter(|link| self.can_read(link))
            .collect()
    }

    fn query_links(
        &mut self,
        index: Option<u32>,
        source: Option<u32>,
        target: Option<u32>,
    ) -> Vec<Link> {
        let links = self.links.query_links(index, source, target);
        links
            .into_iter()
            .filter(|link| self.can_read(link))
            .collect()
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        let found = self.links.search(source, target)?;
        self.exists(found).then_some(found)
    }

    /// Panics when the subject may not create; use
    /// [`try_get_or_create`](NamedTypeLinks::try_get_or_create).
    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        Self::panic_on(self.try_get_or_create(source, target))
    }

    /// The name of a link the subject may refer to; any other has none.
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        if !self.can_reference(id) {
            return Ok(None);
        }
        self.links.get_name(id)
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        self.require_writable(id, "set-name")?;
        if let Some(holder) = self.links.get_by_name(name)? {
            if holder != 0 && holder != id {
                if let Some(link) = self.links.get_link(holder) {
                    if !self.can_write(&link) {
                        // Taking the name takes it from its link.
                        return Err(Forbidden::new(format!("set-name {id}")).into());
                    }
                }
            }
        }
        self.links.set_name(id, name)
    }

    /// The link of a name, when the subject may refer to it; any other name
    /// is an unknown name.
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        let found = self.links.get_by_name(name)?;
        Ok(found.filter(|&id| id == 0 || self.can_reference(id)))
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        if let Some(link) = self.links.get_link(id) {
            if !self.can_read(&link) {
                // Nothing the subject sees has a name to remove.
                return Ok(());
            }
            self.require_writable(id, "remove-name")?;
        }
        self.links.remove_name(id)
    }

    fn save(&mut self) -> Result<()> {
        self.links.save()
    }
}
