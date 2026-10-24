use super::{error, journal, Engine, IsolationLevel, Row, Snapshot};
use crate::{GenericLink, LinkError, LinksStorage, TransitionLogStore};
use doublets::data::LinkReference;
use std::collections::BTreeMap;
use std::sync::Arc;

/// An isolated write set with the same raw LinksStorage API as other backends.
/// Dropping or rolling back discards the write set; allocation may leave gaps.
pub struct EngineTransaction<T: LinkReference, L: TransitionLogStore> {
    pub(super) engine: Engine<T, L>,
    pub(super) base: Snapshot<T>,
    pub(super) writes: BTreeMap<T, Option<Row<T>>>,
    pub(super) closed: bool,
    pub(super) isolation: IsolationLevel,
}
impl<T: LinkReference, L: TransitionLogStore> EngineTransaction<T, L> {
    fn check_open(&self) -> Result<(), LinkError> {
        if self.closed {
            Err(error("transaction is closed"))
        } else {
            Ok(())
        }
    }
    fn row(&self, id: T) -> Option<Row<T>> {
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
    fn view(&self) -> BTreeMap<T, Row<T>> {
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
            self.closed = true;
            self.writes.clear();
        }
    }
    pub fn commit(&mut self) -> Result<u64, LinkError> {
        self.check_open()?;
        let mut state = self.engine.lock()?;
        // Every commit attempt ends this session, including conflicts/failures.
        state.active -= 1;
        self.closed = true;
        let writes = std::mem::take(&mut self.writes);
        if self.isolation == IsolationLevel::Serializable
            && state.current.revision != self.base.revision
        {
            return Err(error(
                "conflict: database revision changed (serializable validation)",
            ));
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
        state.validate(&rows)?;
        let revision = state
            .current
            .revision
            .checked_add(1)
            .ok_or_else(|| error("revision overflow"))?;
        let snapshot = Snapshot {
            revision,
            rows: Arc::new(rows),
        };
        let record = journal::encode(&snapshot, state.next);
        // Any I/O error may follow a successful write. Reopen to discover the
        // outcome; never reuse a possibly duplicated revision on this handle.
        if let Err(cause) = state
            .log
            .append_log_entry(&record)
            .and_then(|()| state.log.flush_log())
        {
            state.poisoned = true;
            return Err(error(&format!(
                "indeterminate commit: {cause}; reopen the engine"
            )));
        }
        for id in writes.keys() {
            state.changed.insert(*id, revision);
        }
        state.current = snapshot.clone();
        state.retain();
        if state.options.trace {
            eprintln!(
                "[Engine] committed revision={revision} writes={}",
                writes.len()
            );
        }
        self.base = snapshot;
        self.writes.clear();
        Ok(revision)
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
            let id =
                T::try_from(state.next).map_err(|_| LinkError::AddressOutOfRange(state.next))?;
            state.next = state
                .next
                .checked_add(1)
                .ok_or_else(|| error("address space exhausted"))?;
            id
        };
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
            state.next = state.next.max(
                address
                    .checked_add(1)
                    .ok_or_else(|| error("address space exhausted"))?,
            );
            self.writes.insert(
                id,
                Some(Row {
                    link: GenericLink::new(id, T::from_byte(0), T::from_byte(0)),
                    name: None,
                }),
            );
        }
        Ok(id)
    }
    fn get_link(&self, id: T) -> Option<GenericLink<T>> {
        self.row(id).map(|row| row.link)
    }
    fn update_link(&mut self, id: T, source: T, target: T) -> Result<GenericLink<T>, LinkError> {
        self.check_open()?;
        let mut row = self.row(id).ok_or_else(|| LinkError::not_found(id))?;
        let previous = row.link;
        row.link = GenericLink::new(id, source, target);
        self.writes.insert(id, Some(row));
        Ok(previous)
    }
    fn delete_link(&mut self, id: T) -> Result<GenericLink<T>, LinkError> {
        self.check_open()?;
        let previous = self.row(id).ok_or_else(|| LinkError::not_found(id))?.link;
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
