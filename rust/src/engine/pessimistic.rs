//! Optional engine-local point-write ownership: fails fast, or waits a bounded
//! time and refuses a wait that would close a cycle.
use super::{error, State};
use crate::LinkError;
use doublets::data::LinkReference;
use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, MutexGuard};
use std::time::{Duration, Instant};

/// Exists only for engines that selected `lock_wait`.
pub(super) struct LockWaits {
    timeout: Duration,
    released: Arc<Condvar>,
    /// A session waits for one owner at a time. An owner's release removes the
    /// edges to it, so an edge never outlives the session it names.
    waiting_for: BTreeMap<usize, usize>,
}

impl LockWaits {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            released: Arc::new(Condvar::new()),
            waiting_for: BTreeMap::new(),
        }
    }

    fn closes_cycle(&self, waiter: usize, holder: usize) -> bool {
        let mut next = Some(holder);
        while let Some(session) = next {
            if session == waiter {
                return true;
            }
            next = self.waiting_for.get(&session).copied();
        }
        false
    }

    /// A group commit releases on a private working state, then wakes everyone
    /// here: each waiter checks again and records its edge if it still waits.
    pub fn wake_all(&mut self) {
        self.waiting_for.clear();
        self.released.notify_all();
    }

    fn release(&mut self, holder: usize) {
        self.waiting_for.retain(|_, owner| *owner != holder);
        self.released.notify_all();
    }
}

#[derive(Default)]
pub(super) struct WriteLocks<T> {
    owner: Arc<()>,
    keys: Vec<T>,
    blocker: Option<usize>,
}

fn token(owner: &Arc<()>) -> usize {
    Arc::as_ptr(owner) as usize
}

impl<T: LinkReference> WriteLocks<T> {
    pub fn token(&self) -> usize {
        token(&self.owner)
    }

    /// The owner that refused this session's last write, if not asked since.
    pub fn take_blocker(&mut self) -> Option<usize> {
        self.blocker.take()
    }

    /// Without `lock_wait` this returns at once. Otherwise the mutex is given up
    /// until `id` has no other owner, and the caller reads state only afterwards.
    pub fn wait_turn<'a, L>(
        &mut self,
        mut state: MutexGuard<'a, State<T, L>>,
        id: T,
    ) -> Result<MutexGuard<'a, State<T, L>>, LinkError> {
        let Some(timeout) = state.lock_waits.as_ref().map(|waits| waits.timeout) else {
            return Ok(state);
        };
        let me = self.token();
        let started = Instant::now();
        loop {
            let Some(holder) = state.write_owners.get(&id).map(token).filter(|h| *h != me) else {
                return Ok(state);
            };
            let waits = state.lock_waits.as_mut().expect("selected above");
            if waits.closes_cycle(me, holder) {
                return Err(error(
                    "deadlock: waiting for this write address would never end; roll back and retry",
                ));
            }
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(error("busy: timed out waiting for a write address"));
            }
            waits.waiting_for.insert(me, holder);
            let released = waits.released.clone();
            let (guard, _) = released
                .wait_timeout(state, remaining)
                .map_err(|_| error("engine mutex poisoned"))?;
            state = guard;
            state
                .lock_waits
                .as_mut()
                .expect("selected above")
                .waiting_for
                .remove(&me);
            if state.poisoned {
                return Err(error(
                    "indeterminate journal outcome; close and reopen before continuing",
                ));
            }
        }
    }

    /// The caller holds the same mutex as publication. Validate after checking
    /// ownership, including intervening deletes/recreates of identical rows.
    pub fn acquire<L>(
        &mut self,
        state: &mut State<T, L>,
        id: T,
        base_revision: u64,
    ) -> Result<(), LinkError> {
        if let Some(owner) = state.write_owners.get(&id) {
            if !Arc::ptr_eq(owner, &self.owner) {
                self.blocker = Some(token(owner));
                return Err(error("busy: another transaction owns this write address"));
            }
        }
        if state
            .changed
            .get(&id)
            .is_some_and(|revision| *revision > base_revision)
        {
            return Err(error("conflict: a written link changed since begin"));
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = state.write_owners.entry(id) {
            entry.insert(self.owner.clone());
            self.keys.push(id);
        }
        Ok(())
    }

    pub fn release<L>(&mut self, state: &mut State<T, L>) {
        for id in self.keys.drain(..) {
            state.write_owners.remove(&id);
        }
        if let Some(waits) = &mut state.lock_waits {
            waits.release(token(&self.owner));
        }
    }
}
