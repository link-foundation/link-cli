//! Independent branch journals and atomic, row-level three-way merges.
use super::{error, journal, ConcurrencyControl, Engine, EngineOptions, Rows, Snapshot};
use crate::{LinkError, TransitionLogStore};
use doublets::data::LinkReference;
use std::collections::BTreeSet;

pub struct Branch<T: LinkReference, P: TransitionLogStore, L: TransitionLogStore> {
    parent: Engine<T, P>,
    engine: Engine<T, L>,
    base: Snapshot<T>,
    base_next: u128,
}

impl<T: LinkReference, P: TransitionLogStore> Engine<T, P> {
    /// Seed an empty independent log with a flushed copy of the current state.
    /// Branch addresses are independent; divergent allocation collisions conflict.
    pub fn fork<L: TransitionLogStore>(
        &self,
        log: L,
        options: EngineOptions,
    ) -> Result<Branch<T, P, L>, LinkError> {
        let (snapshot, next) = {
            let state = self.lock()?;
            (state.current.clone(), state.next)
        };
        let engine = Engine::open(log, options)?;
        let base = {
            let mut state = engine.lock()?;
            if state.current.revision != 0 {
                return Err(error("fork requires an empty branch log"));
            }
            state.next = next;
            state.publish(
                snapshot.rows.as_ref().clone(),
                snapshot.rows.keys().copied(),
            )?
        };
        Ok(Branch {
            parent: self.clone(),
            engine,
            base,
            base_next: next,
        })
    }

    /// Restore a branch using a merge-base record saved by its previous owner.
    /// The caller must associate that descriptor with the same logical parent.
    /// A descriptor must identify a real committed revision of the branch log.
    pub fn resume_branch<L: TransitionLogStore>(
        &self,
        mut log: L,
        options: EngineOptions,
        merge_base_record: &str,
    ) -> Result<Branch<T, P, L>, LinkError> {
        drop(self.lock()?);
        let (mut base, next) = journal::decode::<T>(merge_base_record)?;
        let mut found = false;
        let mut previous = Snapshot {
            revision: 0,
            rows: std::sync::Arc::new(Rows::new(options.state_structure)),
        };
        let mut previous_next = 1;
        for (index, record) in log.read_log_entries()?.into_iter().enumerate() {
            let (snapshot, allocator) = if index == 0 && record.starts_with("engine3|") {
                journal::decode_checkpoint(&record, options.state_structure)?
            } else {
                journal::decode_after(&record, &previous, previous_next, options.state_structure)?
            };
            if snapshot.revision == base.revision && snapshot.rows == base.rows && allocator == next
            {
                found = true;
            }
            previous = snapshot;
            previous_next = allocator;
        }
        if !found {
            return Err(error("merge base is not a committed branch revision"));
        }
        let structure = options.state_structure;
        let engine = Engine::open(log, options)?;
        base.rows =
            std::sync::Arc::new(std::sync::Arc::unwrap_or_clone(base.rows).adapt(structure, None));
        Ok(Branch {
            parent: self.clone(),
            engine,
            base,
            base_next: next,
        })
    }
}

impl<T: LinkReference, P: TransitionLogStore, L: TransitionLogStore> Branch<T, P, L> {
    pub fn engine(&self) -> &Engine<T, L> {
        &self.engine
    }
    pub fn into_engine(self) -> Engine<T, L> {
        self.engine
    }

    /// Persist this descriptor with application metadata after each merge.
    /// Reusing an older descriptor is conservative: overlapping changes conflict.
    pub fn merge_base_record(&self) -> String {
        journal::encode(&self.base, self.base_next)
    }

    /// Merge the branch's current committed snapshot; unfinished sessions are private.
    /// All constraints, history, flush, poisoning and conflict rules still apply.
    pub fn merge(&mut self) -> Result<u64, LinkError> {
        let (incoming, next) = {
            let state = self.engine.lock()?;
            (state.current.clone(), state.committed_next)
        };
        let mut state = self.parent.lock()?;
        if state.options.concurrency == ConcurrencyControl::Exclusive && state.active > 0 {
            return Err(error("busy: an exclusive transaction is active"));
        }
        let keys: BTreeSet<_> = self
            .base
            .rows
            .keys()
            .chain(incoming.rows.keys())
            .copied()
            .collect();
        let mut rows = state.current.rows.as_ref().clone();
        let mut changes = Vec::new();
        for id in keys {
            let before = self.base.rows.get(&id);
            let after = incoming.rows.get(&id);
            if before == after {
                continue;
            }
            let current = rows.get(&id);
            if current != before && current != after {
                return Err(error("conflict: branch and parent changed the same row"));
            }
            if current == after {
                continue;
            }
            if state.write_owners.contains_key(&id) {
                return Err(error("busy: another transaction owns this write address"));
            }
            match after {
                Some(row) => {
                    rows.insert(id, row.clone());
                }
                None => {
                    rows.remove(&id);
                }
            }
            changes.push(id);
        }
        state.next = state.next.max(next);
        let revision = if changes.is_empty() && state.next == state.committed_next {
            state.current.revision
        } else {
            state.publish(rows, changes)?.revision
        };
        self.base = incoming;
        self.base_next = next;
        Ok(revision)
    }
}
