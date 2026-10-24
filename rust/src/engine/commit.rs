use super::{error, journal, Row, Snapshot, State};
use crate::{LinkError, TransitionLogStore};
use doublets::data::LinkReference;
use std::{collections::BTreeMap, sync::Arc};

impl<T: LinkReference, L: TransitionLogStore> State<T, L> {
    /// Caller holds the engine mutex. Used by sessions and atomic branch merges.
    pub(super) fn publish(
        &mut self,
        rows: BTreeMap<T, Row<T>>,
        changed: impl IntoIterator<Item = T>,
    ) -> Result<Snapshot<T>, LinkError> {
        self.validate(&rows)?;
        let revision = self
            .current
            .revision
            .checked_add(1)
            .ok_or_else(|| error("revision overflow"))?;
        let snapshot = Snapshot {
            revision,
            rows: Arc::new(rows),
        };
        let record = journal::encode(&snapshot, self.next);
        // Failure can follow a completed write. Reopen to discover its outcome.
        if let Err(cause) = self
            .log
            .append_log_entry(&record)
            .and_then(|()| self.log.flush_log())
        {
            self.poisoned = true;
            return Err(error(&format!(
                "indeterminate commit: {cause}; reopen the engine"
            )));
        }
        let mut count = 0;
        for id in changed {
            self.changed.insert(id, revision);
            count += 1;
        }
        self.current = snapshot.clone();
        self.committed_next = self.next;
        self.retain();
        if self.options.trace {
            eprintln!("[Engine] committed revision={revision} writes={count}");
        }
        Ok(snapshot)
    }
}
