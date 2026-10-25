//! Explicit name writes are journaled before changing the logical name map.
use super::*;

impl<T, S, L> GenericTransactionsDecorator<T, S, L>
where
    T: LinkReference,
    S: LinksStorage<T>,
    L: TransitionLogStore,
{
    pub fn set_name(&mut self, index: T, name: &str) -> Result<T, LinkError> {
        self.write_name(index, Some(name))
    }

    pub fn remove_name(&mut self, index: T) -> Result<(), LinkError> {
        self.write_name(index, None).map(|_| ())
    }

    fn write_name(&mut self, index: T, name: Option<&str>) -> Result<T, LinkError> {
        self.ensure_write_allowed()?;
        let before = self
            .inner
            .capture_names()?
            .ok_or_else(|| LinkError::StorageError("storage does not support names".into()))?;
        let mut after = before.clone();
        if let Some(name) = name {
            after.retain(|id, previous| *id == index || previous != name);
            after.insert(index, name.to_owned());
        } else {
            after.remove(&index);
        }
        let mut ids: Vec<_> = before.keys().chain(after.keys()).copied().collect();
        ids.sort_unstable();
        ids.dedup();
        let names = ids
            .into_iter()
            .filter_map(|id| {
                let previous = before.get(&id).cloned();
                let next = after.get(&id).cloned();
                (previous != next).then_some(NameChange {
                    index: id,
                    before: previous,
                    after: next,
                })
            })
            .collect::<Vec<_>>();
        if names.is_empty() {
            return self.inner.set_link_name(index, name);
        }
        let owns = self.ensure_open_transaction()?;
        let record = JournalRecord {
            transition: GenericTransition {
                transaction_id: 0,
                sequence: 0,
                timestamp_ms: 0,
                kind: TransitionKind::Update,
                before: GenericDoubletLink::empty(),
                after: GenericDoubletLink::empty(),
            },
            name_only: true,
            names,
        };
        self.record_records(std::iter::once(record), owns)?;
        // A name's undo record must survive before a name sidecar can change.
        // Keep an explicit owner rollback-only when this pre-effect flush fails.
        let result = self
            .log_store
            .flush_log()
            .and_then(|_| self.inner.set_link_name(index, name));
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.current
                    .as_mut()
                    .expect("open transaction")
                    .rollback_only = true;
                if owns {
                    self.rollback()?;
                }
                return Err(error);
            }
        };
        if owns {
            self.commit_current()?;
        }
        Ok(result)
    }

    pub(super) fn replay_names(
        &mut self,
        record: &JournalRecord<T>,
        undo: bool,
    ) -> Result<(), LinkError> {
        if record.names.is_empty() {
            return Ok(());
        }
        // Clear every changed owner first; otherwise a moved name can erase
        // the owner that was restored earlier in address order.
        for change in &record.names {
            self.inner.set_link_name(change.index, None)?;
        }
        for change in &record.names {
            let value = if undo { &change.before } else { &change.after };
            if let Some(name) = value {
                self.inner.set_link_name(change.index, Some(name))?;
            }
        }
        Ok(())
    }
}
