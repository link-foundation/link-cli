use super::{
    error, validation::ReadSet, Engine, IsolationLevel, Row, Rows, SerializableValidation,
    Snapshot, State,
};
use crate::{GenericLink, LinkError, LinksStorage, TransitionLogStore};
use doublets::data::LinkReference;
use std::{collections::BTreeMap, sync::Mutex};

/// An isolated write set with the same raw LinksStorage API as other backends.
/// Dropping or rolling back discards the write set; allocation may leave gaps.
pub struct EngineTransaction<T: LinkReference, L: TransitionLogStore> {
    pub(super) engine: Engine<T, L>,
    pub(super) base: Snapshot<T>,
    pub(super) writes: BTreeMap<T, Option<Row<T>>>,
    pub(super) reserved: std::collections::VecDeque<T>,
    pub(super) closed: bool,
    pub(super) isolation: IsolationLevel,
    pub(super) reads: Option<Mutex<ReadSet<T>>>,
    pub(super) locks: Option<super::pessimistic::WriteLocks<T>>,
}
impl<T: LinkReference, L: TransitionLogStore> EngineTransaction<T, L> {
    fn check_open(&self) -> Result<(), LinkError> {
        if self.closed {
            Err(error("transaction is closed"))
        } else {
            Ok(())
        }
    }
    fn acquire_write(&mut self, id: T) -> Result<(), LinkError> {
        if let Some(locks) = &mut self.locks {
            let mut state = self.engine.lock()?;
            locks.acquire(&mut state, id, self.base.revision)?;
        }
        Ok(())
    }
    fn row(&self, id: T) -> Option<Row<T>> {
        if !self.closed {
            if let Some(reads) = &self.reads {
                reads
                    .lock()
                    .expect("read-set mutex poisoned")
                    .keys
                    .insert(id);
            }
        }
        if let Some(row) = self.writes.get(&id) {
            return row.clone();
        }
        if self.isolation == IsolationLevel::ReadCommitted && !self.closed {
            return self
                .engine
                .state
                .lock()
                .ok()?
                .current
                .rows
                .get(&id)
                .cloned();
        }
        self.base.rows.get(&id).cloned()
    }
    fn view(&self) -> Rows<T> {
        if !self.closed {
            if let Some(reads) = &self.reads {
                reads.lock().expect("read-set mutex poisoned").scanned = true;
            }
        }
        let mut rows = if self.isolation == IsolationLevel::ReadCommitted && !self.closed {
            self.engine
                .state
                .lock()
                .expect("engine mutex poisoned")
                .current
                .rows
                .as_ref()
                .clone()
        } else {
            self.base.rows.as_ref().clone()
        };
        for (id, row) in &self.writes {
            match row {
                Some(row) => {
                    rows.insert(*id, row.clone());
                }
                None => {
                    rows.remove(id);
                }
            }
        }
        rows
    }
    pub fn get_name(&self, id: T) -> Option<String> {
        self.row(id).and_then(|row| row.name)
    }
    pub fn set_name(&mut self, id: T, name: Option<String>) -> Result<(), LinkError> {
        self.check_open()?;
        let mut row = self.row(id).ok_or_else(|| LinkError::not_found(id))?;
        self.acquire_write(id)?;
        row.name = name;
        self.writes.insert(id, Some(row));
        Ok(())
    }
    pub fn get_by_name(&self, name: &str) -> Option<T> {
        self.view()
            .values()
            .find(|row| row.name.as_deref() == Some(name))
            .map(|row| row.link.index)
    }
    pub fn rollback(&mut self) {
        if !self.closed {
            let mut state = self
                .engine
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.active -= 1;
            if let Some(mut locks) = self.locks.take() {
                locks.release(&mut state);
            }
            if state.active == 0 {
                state.changed.clear();
            }
            self.closed = true;
            self.writes.clear();
            self.reserved.clear();
            self.reads.take();
        }
    }
    pub fn commit(&mut self) -> Result<u64, LinkError> {
        self.check_open()?;
        let engine = self.engine.clone();
        let mut state = match engine.lock() {
            Ok(state) => state,
            Err(cause) => {
                self.rollback();
                return Err(cause);
            }
        };
        self.commit_locked(&mut state)
    }

    // Group preparation uses the same validation against private tentative
    // state, with a memory journal instead of the engine's physical log.
    pub(super) fn commit_locked<M: TransitionLogStore>(
        &mut self,
        state: &mut State<T, M>,
    ) -> Result<u64, LinkError> {
        self.check_open()?;
        // Every commit attempt ends this session, including conflicts/failures.
        state.active -= 1;
        self.closed = true;
        self.reserved.clear();
        // Publication holds this mutex until completion, so no writer can
        // acquire released addresses yet. Release before fallible callbacks
        // also avoids retaining ownership if a custom log panics.
        if let Some(mut locks) = self.locks.take() {
            locks.release(state);
        }
        let writes = std::mem::take(&mut self.writes);
        let reads = self.reads.take().map(Mutex::into_inner);
        let result = (|| {
            let reads = reads
                .transpose()
                .map_err(|_| error("read-set mutex poisoned"))?;
            if self.isolation == IsolationLevel::Serializable
                && (state.options.serializable_validation == SerializableValidation::Revision
                    || reads.as_ref().is_some_and(|reads| reads.scanned))
                && state.current.revision != self.base.revision
            {
                return Err(error(
                    "conflict: database revision changed (serializable validation)",
                ));
            }
            if reads.as_ref().is_some_and(|reads| {
                reads.keys.iter().any(|id| {
                    state
                        .changed
                        .get(id)
                        .is_some_and(|revision| *revision > self.base.revision)
                })
            }) {
                return Err(error("conflict: a read link changed since begin"));
            }
            if writes.keys().any(|id| {
                state
                    .changed
                    .get(id)
                    .is_some_and(|revision| *revision > self.base.revision)
            }) {
                return Err(error("conflict: a written link changed since begin"));
            }
            if writes.is_empty() {
                return Ok(state.current.revision);
            }
            let mut rows = state.current.rows.as_ref().clone();
            for (id, row) in &writes {
                match row {
                    Some(row) => {
                        rows.insert(*id, row.clone());
                    }
                    None => {
                        rows.remove(id);
                    }
                }
            }
            self.base = state.publish(rows, writes.keys().copied())?;
            self.writes.clear();
            Ok(self.base.revision)
        })();
        // Validate the last committer before reclaiming generations: otherwise
        // an intervening delete/recreate could be mistaken for an unchanged row.
        if state.active == 0 {
            state.changed.clear();
        }
        result
    }
}
impl<T: LinkReference, L: TransitionLogStore> Drop for EngineTransaction<T, L> {
    fn drop(&mut self) {
        self.rollback();
    }
}

impl<T: LinkReference, L: TransitionLogStore> LinksStorage<T> for EngineTransaction<T, L> {
    fn create_link(&mut self, source: T, target: T) -> Result<T, LinkError> {
        self.check_open()?;
        let id = {
            let mut state = self.engine.lock()?;
            let (id, next) = if let Some(id) = self.reserved.front().copied() {
                (id, state.next)
            } else {
                let id = T::try_from(state.next)
                    .map_err(|_| LinkError::AddressOutOfRange(state.next))?;
                let next = state
                    .next
                    .checked_add(1)
                    .ok_or_else(|| error("address space exhausted"))?;
                (id, next)
            };
            if let Some(locks) = &mut self.locks {
                locks.acquire(&mut state, id, self.base.revision)?;
            }
            state.next = next;
            id
        };
        if self.reserved.front() == Some(&id) {
            self.reserved.pop_front();
        }
        self.writes.insert(
            id,
            Some(Row {
                link: GenericLink::new(id, source, target),
                name: None,
            }),
        );
        Ok(id)
    }
    fn ensure_link_created(&mut self, id: T) -> Result<T, LinkError> {
        self.check_open()?;
        if id == T::from_byte(0) {
            return Err(error("zero is not a stored address"));
        }
        if self.row(id).is_none() {
            let address: u128 = id
                .try_into()
                .map_err(|_| error("address conversion failed"))?;
            let mut state = self.engine.lock()?;
            let next = state.next.max(
                address
                    .checked_add(1)
                    .ok_or_else(|| error("address space exhausted"))?,
            );
            if let Some(locks) = &mut self.locks {
                locks.acquire(&mut state, id, self.base.revision)?;
            }
            state.next = next;
            self.writes.insert(
                id,
                Some(Row {
                    link: GenericLink::new(id, T::from_byte(0), T::from_byte(0)),
                    name: None,
                }),
            );
        }
        self.reserved.retain(|pending| *pending != id);
        Ok(id)
    }
    fn get_link(&self, id: T) -> Option<GenericLink<T>> {
        self.row(id).map(|row| row.link)
    }
    fn update_link(&mut self, id: T, source: T, target: T) -> Result<GenericLink<T>, LinkError> {
        self.check_open()?;
        let mut row = self.row(id).ok_or_else(|| LinkError::not_found(id))?;
        self.acquire_write(id)?;
        let previous = row.link;
        row.link = GenericLink::new(id, source, target);
        self.writes.insert(id, Some(row));
        Ok(previous)
    }
    fn delete_link(&mut self, id: T) -> Result<GenericLink<T>, LinkError> {
        self.check_open()?;
        let previous = self.row(id).ok_or_else(|| LinkError::not_found(id))?.link;
        self.acquire_write(id)?;
        self.writes.insert(id, None);
        Ok(previous)
    }
    fn all_links(&self) -> Vec<GenericLink<T>> {
        self.view().values().map(|row| row.link).collect()
    }
    /// Flush commits the staged transaction, rather than exposing uncommitted data.
    fn flush(&mut self) -> Result<(), LinkError> {
        self.commit().map(|_| ())
    }
}
