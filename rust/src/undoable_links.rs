//! Writes kept to be undone (issue #149): [`LinksUndoLog`] and the
//! [`UndoableLinksDecorator`] that fills it, so a request that fails can be
//! undone whole. The C# counterparts are `LinksUndoLog<TLinkAddress>` and
//! `UndoableLinksDecorator<TLinkAddress>`.

use std::collections::HashSet;

use anyhow::Result;

use crate::link::Link;
use crate::link_storage::ChangeObserver;
use crate::named_type_links::NamedTypeLinks;

/// A change as a store reports it, `None` standing for no link: `(None, link)`
/// is a creation, `(link, None)` a deletion.
pub type LinkChange = (Option<Link>, Option<Link>);

fn present(link: Link) -> Option<Link> {
    (!link.is_null()).then_some(link)
}

/// The writes made to a store, kept to undo them: every change a write
/// reports, and the names the links had before.
///
/// Undoing goes back through the changes: it deletes what was created,
/// updates back what was updated and creates again, at the same address,
/// what was deleted. Names are not changes: a deleted link loses its name
/// before anyone above the names store hears of the deletion, so the names a
/// write may touch are kept before it, with [`keep_name`](Self::keep_name)
/// and [`keep_names_of_usages`](Self::keep_names_of_usages).
#[derive(Debug, Default, Clone)]
pub struct LinksUndoLog {
    changes: Vec<LinkChange>,
    names: Vec<(u32, Option<String>)>,
    named: HashSet<u32>,
}

impl LinksUndoLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether nothing is kept.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty() && self.names.is_empty()
    }

    /// The changes kept, in the order they were made.
    pub fn changes(&self) -> &[LinkChange] {
        &self.changes
    }

    /// Keeps one change; a change that changes nothing is not kept.
    pub fn record(&mut self, before: Option<Link>, after: Option<Link>) {
        if before.is_some() && before == after {
            return;
        }
        self.changes.push((before, after));
    }

    /// Keeps one change as a [`ChangeObserver`] reports it: the null link is
    /// no link.
    pub fn record_observed(&mut self, before: Link, after: Link) {
        self.record(present(before), present(after));
    }

    /// Keeps the name `link` has now, unless one of it is kept already.
    pub fn keep_name<L: NamedTypeLinks + ?Sized>(
        &mut self,
        names: &mut L,
        link: u32,
    ) -> Result<()> {
        if self.named.insert(link) {
            let name = names.get_name(link)?;
            self.names.push((link, name));
        }
        Ok(())
    }

    /// Keeps the name of the link called `name` now, if any.
    pub fn keep_holder_of<L: NamedTypeLinks + ?Sized>(
        &mut self,
        links: &mut L,
        name: &str,
    ) -> Result<()> {
        match links.get_by_name(name)? {
            Some(holder) if holder != 0 => self.keep_name(links, holder),
            _ => Ok(()),
        }
    }

    /// Keeps the names of `link` and of every link that uses it, directly or
    /// not: the links deleting it deletes, or merging it repoints.
    pub fn keep_names_of_usages<L: NamedTypeLinks + ?Sized>(
        &mut self,
        links: &mut L,
        link: u32,
    ) -> Result<()> {
        for usage in usages_of(links, link) {
            self.keep_name(links, usage)?;
        }
        Ok(())
    }

    /// Forgets everything kept: the writes stay.
    pub fn clear(&mut self) {
        self.changes.clear();
        self.names.clear();
        self.named.clear();
    }

    /// Undoes every write kept, on `links` (the store the writes were made
    /// on, not a view over it), and forgets them.
    ///
    /// Returns the addresses of the links the writes created, now deleted again.
    pub fn undo<L: NamedTypeLinks + ?Sized>(&mut self, links: &mut L) -> Result<Vec<u32>> {
        let mut created = Vec::new();
        for &(before, after) in self.changes.iter().rev() {
            match (before, after) {
                (None, Some(made)) => {
                    if links.exists(made.index) {
                        // Whatever used it is undone already.
                        links.delete(made.index)?;
                    }
                    created.push(made.index);
                }
                (Some(was), None) => {
                    if !links.exists(was.index) {
                        links.try_ensure_created(was.index)?;
                    }
                    if was.source != 0 || was.target != 0 {
                        links.update(was.index, was.source, was.target)?;
                    }
                }
                (Some(old), Some(_)) => {
                    if links.exists(old.index) {
                        links.update(old.index, old.source, old.target)?;
                    }
                }
                (None, None) => {}
            }
        }
        // A name is the link's again; first take every name a write gave.
        for (link, name) in &self.names {
            if *name != links.get_name(*link)? && (name.is_none() || !links.exists(*link)) {
                links.remove_name(*link)?;
            }
        }
        for (link, name) in &self.names {
            if let Some(name) = name {
                if links.exists(*link) && Some(name) != links.get_name(*link)?.as_ref() {
                    links.set_name(*link, name)?;
                }
            }
        }
        self.clear();
        Ok(created)
    }
}

/// `link` and every link that uses it, directly or not, `link` first.
pub fn usages_of<L: NamedTypeLinks + ?Sized>(links: &mut L, link: u32) -> Vec<u32> {
    let mut found = vec![link];
    let mut seen = HashSet::from([link]);
    let mut i = 0;
    while i < found.len() {
        let at = found[i];
        let usages = links
            .query_links(None, Some(at), None)
            .into_iter()
            .chain(links.query_links(None, None, Some(at)));
        for usage in usages {
            if seen.insert(usage.index) {
                found.push(usage.index);
            }
        }
        i += 1;
    }
    found
}

/// Keeps the names an update of `index` to `(source target)` may take: of
/// the link, and of its usages too when it merges into an equal link and so
/// is deleted.
pub fn keep_names_before_update<L: NamedTypeLinks + ?Sized>(
    log: &mut LinksUndoLog,
    links: &mut L,
    index: u32,
    source: u32,
    target: u32,
) -> Result<()> {
    match links.search(source, target) {
        Some(equal) if equal != 0 && equal != index => log.keep_names_of_usages(links, index),
        _ => log.keep_name(links, index),
    }
}

/// A store that keeps every write in a [`LinksUndoLog`] while
/// [`recording`](Self::recording), so a request that fails can be undone
/// whole. The links server puts it under every session with permissions.
pub struct UndoableLinksDecorator<L: NamedTypeLinks> {
    links: L,
    log: LinksUndoLog,
    recording: bool,
}

impl<L: NamedTypeLinks> UndoableLinksDecorator<L> {
    pub fn new(links: L) -> Self {
        Self {
            links,
            log: LinksUndoLog::new(),
            recording: false,
        }
    }

    /// The writes kept since [`begin`](Self::begin).
    pub fn log(&self) -> &LinksUndoLog {
        &self.log
    }

    /// Whether writes are kept.
    pub fn recording(&self) -> bool {
        self.recording
    }

    /// Starts keeping writes, forgetting the ones kept before.
    pub fn begin(&mut self) {
        self.log.clear();
        self.recording = true;
    }

    /// Stops keeping writes; they stay.
    pub fn commit(&mut self) {
        self.recording = false;
        self.log.clear();
    }

    /// Undoes every write kept since [`begin`](Self::begin).
    ///
    /// Returns the addresses of the links the writes created, now deleted again.
    pub fn rollback(&mut self) -> Result<Vec<u32>> {
        self.recording = false;
        self.log.undo(&mut self.links)
    }

    pub fn inner(&self) -> &L {
        &self.links
    }

    pub fn inner_mut(&mut self) -> &mut L {
        &mut self.links
    }

    pub fn into_inner(self) -> L {
        self.links
    }

    fn created(&mut self, id: u32) -> u32 {
        if self.recording {
            let link = self.links.get_link(id).unwrap_or(Link::new(id, 0, 0));
            self.log.record(None, Some(link));
        }
        id
    }
}

impl<L: NamedTypeLinks> NamedTypeLinks for UndoableLinksDecorator<L> {
    fn query_limits(&self) -> Option<crate::query_options::QueryLimits> {
        self.links.query_limits()
    }

    fn reserve_query_allocations(
        &mut self,
        processor: &crate::query_processor::QueryProcessor,
        restriction: &crate::lino_link::LinoLink,
        substitution: &crate::lino_link::LinoLink,
    ) -> Result<Option<std::collections::HashSet<u32>>> {
        self.links
            .reserve_query_allocations(processor, restriction, substitution)
    }

    fn create(&mut self, source: u32, target: u32) -> u32 {
        let id = self.links.create(source, target);
        self.created(id)
    }

    fn ensure_created(&mut self, id: u32) -> u32 {
        if id == 0 || self.links.exists(id) {
            return id;
        }
        let id = self.links.ensure_created(id);
        self.created(id)
    }

    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        if id != 0 && self.links.exists(id) {
            return Ok(id);
        }
        let id = self.links.try_ensure_created(id)?;
        Ok(self.created(id))
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.links.get_link(id)
    }

    fn exists(&mut self, id: u32) -> bool {
        self.links.exists(id)
    }

    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        let id = self.links.try_create(source, target)?;
        Ok(self.created(id))
    }

    fn reference_exists(&mut self, id: u32) -> bool {
        self.links.reference_exists(id)
    }

    fn next_created_addresses(&mut self, count: usize) -> Result<Vec<u32>> {
        // Created and deleted again: nothing to undo.
        self.links.next_created_addresses(count)
    }

    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        if !self.recording {
            return self.links.update_observed(id, source, target, observer);
        }
        keep_names_before_update(&mut self.log, &mut self.links, id, source, target)?;
        let log = &mut self.log;
        self.links
            .update_observed(id, source, target, &mut |before, after| {
                log.record_observed(before, after);
                observer(before, after);
            })
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        if !self.recording {
            return self.links.delete_observed(id, observer);
        }
        self.log.keep_names_of_usages(&mut self.links, id)?;
        let log = &mut self.log;
        self.links.delete_observed(id, &mut |before, after| {
            log.record_observed(before, after);
            observer(before, after);
        })
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
        match self.links.search(source, target) {
            Some(found) => found,
            None => self.create(source, target),
        }
    }

    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        self.links.get_name(id)
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        if self.recording {
            self.log.keep_name(&mut self.links, id)?;
            self.log.keep_holder_of(&mut self.links, name)?;
        }
        self.links.set_name(id, name)
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        self.links.get_by_name(name)
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        if self.recording {
            self.log.keep_name(&mut self.links, id)?;
        }
        self.links.remove_name(id)
    }

    fn save(&mut self) -> Result<()> {
        self.links.save()
    }
}
