//! Reserve an execution preview atomically against the engine allocator.
use super::{query_scratch::QueryScratch, EngineTransaction, IsolationLevel};
use crate::{
    lino_link::LinoLink, query_address::QueryAddress, GenericQueryProcessor, TransitionLogStore,
};
use anyhow::Result;
use std::collections::HashSet;

impl<T: QueryAddress, L: TransitionLogStore> EngineTransaction<T, L> {
    pub(super) fn reserve_query(
        &mut self,
        processor: &GenericQueryProcessor<T>,
        restriction: &LinoLink,
        substitution: &LinoLink,
    ) -> Result<Option<HashSet<T>>> {
        if self.closed {
            anyhow::bail!("transaction is closed");
        }
        let engine = self.engine.clone();
        let mut state = engine.lock()?;
        if let Some(reads) = &self.reads {
            reads
                .lock()
                .map_err(|_| anyhow::anyhow!("read-set mutex poisoned"))?
                .scanned = true;
        }
        let visible = if self.isolation == IsolationLevel::ReadCommitted {
            &state.current
        } else {
            &self.base
        };
        let mut rows = visible
            .rows
            .iter()
            .map(|(id, row)| (*id, row.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        for (id, row) in &self.writes {
            if let Some(row) = row {
                rows.insert(*id, row.clone());
            } else {
                rows.remove(id);
            }
        }
        let maximum: u128 = T::MAX
            .try_into()
            .map_err(|_| anyhow::anyhow!("query address conversion failed"))?;
        let mut scratch = QueryScratch {
            rows,
            next: state.next,
            last: maximum - 1,
            reserved: self.reserved.clone(),
            allocated: Vec::new(),
            fresh_range: None,
            limits: None,
        };
        processor.preview(&mut scratch, restriction, substitution)?;
        state.next = state.next.max(scratch.next);
        for id in scratch.allocated {
            if !self.reserved.contains(&id) {
                self.reserved.push_back(id);
            }
        }
        Ok(Some(scratch.rows.into_keys().collect()))
    }
}
