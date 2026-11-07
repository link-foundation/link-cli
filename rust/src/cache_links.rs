//! A links store used as a cache: links can expire, and the store can be kept
//! to a number of links by evicting the ones used least recently. It has the API
//! of any other store, so only a store that selects it pays for it. See
//! `docs/ENGINE-CACHE.md`. The C# twin is `CacheLinksDecorator<TLinkAddress>`, the
//! JavaScript one `CacheLinks` of `@link-foundation/link-cli-client`.
//!
//! The two rules differ on purpose. A deadline is a promise, so a link that comes
//! due is deleted with whatever the store's own delete takes with it. Capacity only
//! has to free a place, so it evicts a link nothing else uses and takes nothing
//! else along. When every link is in use the create is refused.
//!
//! There is no worker. Everything due is deleted by the next call that touches the
//! store, or by [`CacheLinks::purge_expired`] when a caller wants to run it on a
//! timer. Deadlines live in memory: a store opened again has links without one.

use std::collections::{BTreeSet, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};

use crate::link::Link;
use crate::link_storage::ChangeObserver;
use crate::named_type_links::NamedTypeLinks;

/// What a [`CacheLinks`] enforces. Each is optional, and the default is neither.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheOptions {
    /// How many links the store may hold, reached by evicting.
    pub max_links: Option<usize>,
    /// The milliseconds a link created here has before it comes due.
    pub time_to_live: Option<u64>,
}

fn system_clock() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

struct Entry {
    used: u64,
    deadline: Option<u64>,
}

/// Follows every change the store reports. It is apart from the store below, so
/// a write can report to it while the store is borrowed for that write.
struct Tracker {
    time_to_live: Option<u64>,
    now: u64,
    uses: u64,
    entries: HashMap<u32, Entry>,
    /// How many other links refer to an address.
    usages: HashMap<u32, u32>,
    /// The links nothing else uses, least recently used first.
    evictable: BTreeSet<(u64, u32)>,
    /// The links with a deadline, soonest first.
    deadlines: BTreeSet<(u64, u32)>,
}

impl Tracker {
    fn is_used(&self, id: u32) -> bool {
        self.usages.contains_key(&id)
    }

    fn references(link: &Link) -> impl Iterator<Item = u32> {
        let index = link.index;
        // A link that refers to itself twice uses nothing else, and the null link
        // is no link.
        let target = (link.target != link.source).then_some(link.target);
        std::iter::once(link.source)
            .chain(target)
            .filter(move |&reference| reference != index && reference != 0)
    }

    fn touch(&mut self, id: u32) {
        self.uses += 1;
        let (uses, used) = (self.uses, self.is_used(id));
        if let Some(entry) = self.entries.get_mut(&id) {
            let previous = std::mem::replace(&mut entry.used, uses);
            if !used {
                self.evictable.remove(&(previous, id));
                self.evictable.insert((uses, id));
            }
        }
    }

    fn add_usage(&mut self, id: u32) {
        let usages = self.usages.entry(id).or_default();
        *usages += 1;
        if *usages == 1 {
            if let Some(entry) = self.entries.get(&id) {
                self.evictable.remove(&(entry.used, id));
            }
        }
    }

    fn remove_usage(&mut self, id: u32) {
        let Some(usages) = self.usages.get_mut(&id) else {
            return;
        };
        *usages -= 1;
        if *usages == 0 {
            self.usages.remove(&id);
            if let Some(entry) = self.entries.get(&id) {
                self.evictable.insert((entry.used, id));
            }
        }
    }

    fn insert(&mut self, id: u32, deadline: Option<u64>) {
        self.uses += 1;
        self.entries.insert(
            id,
            Entry {
                used: self.uses,
                deadline,
            },
        );
        if !self.is_used(id) {
            self.evictable.insert((self.uses, id));
        }
        if let Some(deadline) = deadline {
            self.deadlines.insert((deadline, id));
        }
    }

    fn remove(&mut self, id: u32) {
        if let Some(entry) = self.entries.remove(&id) {
            self.evictable.remove(&(entry.used, id));
            if let Some(deadline) = entry.deadline {
                self.deadlines.remove(&(deadline, id));
            }
        }
    }

    fn set_deadline(&mut self, id: u32, deadline: Option<u64>) {
        if let Some(entry) = self.entries.get_mut(&id) {
            if let Some(previous) = std::mem::replace(&mut entry.deadline, deadline) {
                self.deadlines.remove(&(previous, id));
            }
            if let Some(deadline) = deadline {
                self.deadlines.insert((deadline, id));
            }
        }
    }

    /// A change as the store reports it: a null link is no link.
    fn changed(&mut self, before: Link, after: Link) {
        Self::references(&before).for_each(|reference| self.remove_usage(reference));
        Self::references(&after).for_each(|reference| self.add_usage(reference));
        match (before.is_null(), after.is_null()) {
            (false, true) => self.remove(before.index),
            (false, false) => self.touch(before.index),
            (true, _) => {
                let deadline = self
                    .time_to_live
                    .map(|time_to_live| self.now.saturating_add(time_to_live));
                self.insert(after.index, deadline);
            }
        }
    }

    fn next_due(&self) -> Option<u32> {
        let &(deadline, id) = self.deadlines.first()?;
        (deadline <= self.now).then_some(id)
    }

    fn least_recently_used_except(&self, kept: [u32; 2]) -> Option<u32> {
        self.evictable
            .iter()
            .map(|&(_, id)| id)
            .find(|id| !kept.contains(id))
    }
}

/// `links` kept as [`CacheOptions`] tell; see the top of this module. Every write
/// has to go through it for it to know which links exist.
pub struct CacheLinks<L> {
    links: L,
    max_links: Option<usize>,
    clock: Box<dyn FnMut() -> u64 + Send>,
    tracker: Tracker,
}

impl<L: NamedTypeLinks> CacheLinks<L> {
    /// `links` on the system clock. A step back in that clock delays what comes due.
    /// `wasm32-unknown-unknown` has no system clock: there, use [`Self::with_clock`].
    pub fn new(links: L, options: CacheOptions) -> Self {
        Self::with_clock(links, options, system_clock)
    }

    /// `links` on a clock of milliseconds. The links already there are the least
    /// recently used, in the order of their addresses, and have no deadline.
    pub fn with_clock(
        mut links: L,
        options: CacheOptions,
        clock: impl FnMut() -> u64 + Send + 'static,
    ) -> Self {
        let mut tracker = Tracker {
            time_to_live: None,
            now: 0,
            uses: 0,
            entries: HashMap::new(),
            usages: HashMap::new(),
            evictable: BTreeSet::new(),
            deadlines: BTreeSet::new(),
        };
        for link in links.all_links() {
            tracker.changed(Link::null(), link);
        }
        tracker.time_to_live = options.time_to_live;
        Self {
            links,
            max_links: options.max_links,
            clock: Box::new(clock),
            tracker,
        }
    }

    /// The store below, to read without using anything.
    pub fn links(&self) -> &L {
        &self.links
    }

    /// The store below, without the cache.
    pub fn into_inner(self) -> L {
        self.links
    }

    /// Deletes what is due now, and how many links that deleted, cascades included.
    pub fn purge_expired(&mut self) -> Result<usize> {
        self.tracker.now = (self.clock)();
        let mut deleted = 0;
        while let Some(id) = self.tracker.next_due() {
            let tracker = &mut self.tracker;
            self.links.delete_observed(id, &mut |before, after| {
                deleted += usize::from(after.is_null());
                tracker.changed(before, after);
            })?;
        }
        Ok(deleted)
    }

    /// Reads have no way to answer a failure, so one that could not delete what came
    /// due panics. A store that cannot delete has failed the way its `create` does.
    fn purged(&mut self) -> &mut Self {
        self.purge_expired()
            .unwrap_or_else(|error| panic!("{error}"));
        self
    }

    /// Gives `id` a deadline in place of any it had; false when there is no such
    /// link. A time of zero deletes it now, as Redis `EXPIRE` does.
    pub fn expire(&mut self, id: u32, milliseconds: u64) -> Result<bool> {
        self.purge_expired()?;
        if !self.tracker.entries.contains_key(&id) {
            return Ok(false);
        }
        let deadline = self.tracker.now.saturating_add(milliseconds);
        self.tracker.set_deadline(id, Some(deadline));
        self.purge_expired()?;
        Ok(true)
    }

    /// Takes the deadline of `id` away; false when it had none.
    pub fn persist(&mut self, id: u32) -> Result<bool> {
        self.purge_expired()?;
        let had = self
            .tracker
            .entries
            .get(&id)
            .is_some_and(|entry| entry.deadline.is_some());
        self.tracker.set_deadline(id, None);
        Ok(had)
    }

    /// The milliseconds `id` has left; `None` when it has no deadline or is gone.
    pub fn time_to_live(&mut self, id: u32) -> Result<Option<u64>> {
        self.purge_expired()?;
        let now = self.tracker.now;
        Ok(self
            .tracker
            .entries
            .get(&id)
            .and_then(|entry| entry.deadline)
            .map(|deadline| deadline - now))
    }

    /// Frees a place for a link, which will refer to `kept`.
    fn make_room(&mut self, kept: [u32; 2]) -> Result<()> {
        let Some(max_links) = self.max_links else {
            return Ok(());
        };
        while self.tracker.entries.len() >= max_links {
            let Some(victim) = self.tracker.least_recently_used_except(kept) else {
                bail!("the cache holds {max_links} links and every other one is in use");
            };
            let tracker = &mut self.tracker;
            self.links
                .delete_observed(victim, &mut |before, after| tracker.changed(before, after))?;
        }
        Ok(())
    }
}

impl<L: NamedTypeLinks> NamedTypeLinks for CacheLinks<L> {
    fn query_limits(&self) -> Option<crate::query_options::QueryLimits> {
        self.links.query_limits()
    }

    /// Panics when the cache is full; use [`try_create`](NamedTypeLinks::try_create).
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.try_create(source, target)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        self.purge_expired()?;
        self.make_room([source, target])?;
        let id = self.links.try_create(source, target)?;
        self.tracker
            .changed(Link::null(), Link::new(id, source, target));
        Ok(id)
    }

    /// Panics when the cache is full; use
    /// [`try_ensure_created`](NamedTypeLinks::try_ensure_created).
    fn ensure_created(&mut self, id: u32) -> u32 {
        self.try_ensure_created(id)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        self.purge_expired()?;
        if id == 0 || self.tracker.entries.contains_key(&id) {
            return self.links.try_ensure_created(id);
        }
        self.make_room([0, 0])?;
        let created = self.links.try_ensure_created(id)?;
        self.tracker.changed(Link::null(), Link::new(created, 0, 0));
        Ok(created)
    }

    fn reference_exists(&mut self, id: u32) -> bool {
        self.purged().links.reference_exists(id)
    }

    fn claims(&mut self, id: u32) -> Option<bool> {
        self.purged().links.claims(id)
    }

    /// Asked of the store below: asking by creating here could evict.
    fn next_created_addresses(&mut self, count: usize) -> Result<Vec<u32>> {
        self.purge_expired()?;
        self.links.next_created_addresses(count)
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        let link = self.purged().links.get_link(id);
        if link.is_some() {
            self.tracker.touch(id);
        }
        link
    }

    fn exists(&mut self, id: u32) -> bool {
        self.purged().links.exists(id)
    }

    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        self.purge_expired()?;
        let tracker = &mut self.tracker;
        self.links
            .update_observed(id, source, target, &mut |before, after| {
                tracker.changed(before, after);
                observer(before, after);
            })
    }

    fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        self.purge_expired()?;
        let tracker = &mut self.tracker;
        self.links.delete_observed(id, &mut |before, after| {
            tracker.changed(before, after);
            observer(before, after);
        })
    }

    fn all_links(&mut self) -> Vec<Link> {
        self.purged().links.all_links()
    }

    fn query_links(
        &mut self,
        index: Option<u32>,
        source: Option<u32>,
        target: Option<u32>,
    ) -> Vec<Link> {
        self.purged().links.query_links(index, source, target)
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        let found = self.purged().links.search(source, target);
        if let Some(id) = found {
            self.tracker.touch(id);
        }
        found
    }

    /// Panics when the cache is full; use
    /// [`try_get_or_create`](NamedTypeLinks::try_get_or_create).
    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        self.try_get_or_create(source, target)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        self.purge_expired()?;
        self.links.get_name(id)
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        self.purge_expired()?;
        self.links.set_name(id, name)
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        self.purge_expired()?;
        self.links.get_by_name(name)
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        self.purge_expired()?;
        self.links.remove_name(id)
    }

    fn save(&mut self) -> Result<()> {
        self.purge_expired()?;
        self.links.save()
    }
}
