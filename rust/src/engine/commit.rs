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
        let mut changed = changed.into_iter();
        // The same bounded address buffer serves incremental constraints and
        // logical encoding. Unselected engines retain their iterator-only path.
        let buffered = (self.constraint_index.is_some()
            || self.options.revision_encoding == RevisionEncoding::Changes)
            .then(|| changed.by_ref().collect::<Vec<_>>());
        let candidate_index = match &self.constraint_index {
            Some(index) => Some(index.prepare(
                &self.current.rows,
                &rows,
                buffered.as_ref().expect("indexed changes"),
            )?),
            None => {
                self.validate(&rows)?;
                None
            }
        };
        let revision = self
            .current
            .revision
            .checked_add(1)
            .ok_or_else(|| error("revision overflow"))?;
        let snapshot = Snapshot {
            revision,
            rows: Arc::new(rows),
        };
        let record = match self.options.revision_encoding {
            RevisionEncoding::Changes => changes::encode(
                &snapshot,
                self.next,
                &self.current,
                buffered.as_ref().expect("encoded changes"),
            ),
            RevisionEncoding::Snapshot => journal::encode(&snapshot, self.next),
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
            for id in buffered.iter().flatten().copied().chain(changed) {
                if track {
                    self.changed.insert(id, revision);
                }
                count += 1;
            }
        }
        self.current = snapshot.clone();
        self.constraint_index = candidate_index;
        self.committed_next = self.next;
        self.retain();
        if self.options.trace {
            crate::diagnostics::stderr!("[Engine] committed revision={revision} writes={count}");
        }
        Ok(snapshot)
    }
}
