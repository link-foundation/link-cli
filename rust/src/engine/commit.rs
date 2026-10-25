use super::{changes, error, journal, RevisionEncoding, Rows, Snapshot, State};
use crate::{LinkError, TransitionLogStore};
use doublets::data::LinkReference;
use std::sync::Arc;

impl<T: LinkReference, L: TransitionLogStore> State<T, L> {
    /// Caller holds the engine mutex. Used by sessions and atomic branch merges.
    pub(super) fn publish(
        &mut self,
        rows: Rows<T>,
        changed: impl IntoIterator<Item = T>,
    ) -> Result<Snapshot<T>, LinkError> {
        let rows = rows.adapt(self.options.state_structure, None);
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
        let mut changed = changed.into_iter();
        // Retain addresses only for the selected incremental encoder. The
        // default full-image path keeps its existing iterator/cost behavior.
        let encoded = (self.options.revision_encoding == RevisionEncoding::Changes)
            .then(|| changed.by_ref().collect::<Vec<_>>());
        let record = match &encoded {
            Some(writes) => changes::encode(&snapshot, self.next, &self.current, writes),
            None => journal::encode(&snapshot, self.next),
        };
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
        let track = self.active > 0 && self.options.tracks_generations();
        let mut count = 0;
        if track || self.options.trace {
            for id in encoded.iter().flatten().copied().chain(changed) {
                if track {
                    self.changed.insert(id, revision);
                }
                count += 1;
            }
        }
        self.current = snapshot.clone();
        self.committed_next = self.next;
        self.retain();
        if self.options.trace {
            crate::diagnostics::stderr!("[Engine] committed revision={revision} writes={count}");
        }
        Ok(snapshot)
    }
}
