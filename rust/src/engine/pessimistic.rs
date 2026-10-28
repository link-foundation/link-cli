//! Optional engine-local, nonblocking point-write ownership.
use super::{error, State};
use crate::LinkError;
use doublets::data::LinkReference;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct WriteLocks<T> {
    owner: Arc<()>,
    keys: Vec<T>,
}

impl<T: LinkReference> WriteLocks<T> {
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
    }
}
