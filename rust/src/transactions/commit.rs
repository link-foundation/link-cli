//! Retain commit ownership across uncertain decisions and failed checkpoints.
use super::{
    GenericTransactionsDecorator, JournalRecord, LinkError, LinkReference, LinksStorage,
    TransitionLogStore, COMMIT_MARKER_PREFIX,
};

impl<T, S, L> GenericTransactionsDecorator<T, S, L>
where
    T: LinkReference,
    S: LinksStorage<T>,
    L: TransitionLogStore,
{
    pub(super) fn commit_current(&mut self) -> Result<(), LinkError> {
        let Some(current) = self.current.as_ref() else {
            return Ok(());
        };
        if current.rollback_only {
            return Err(LinkError::Transaction(
                "Cannot commit a rollback-only transaction; rollback is required.".into(),
            ));
        }
        let id = current.id;
        if !self.committed.contains(&id) {
            let retry = current.commit_attempted;
            self.current
                .as_mut()
                .expect("transaction is open")
                .commit_attempted = true;
            let marker = format!("{COMMIT_MARKER_PREFIX}{id:032x}");
            // Only uncertain retries read the journal. An append can persist
            // and then fail; avoid another physical decision for the same ID.
            if !retry || !self.log_store.read_log_entries()?.contains(&marker) {
                self.write_marker(&marker)?;
            }
            // A links-backed log buffers appends; a commit is durable only
            // after its explicit flush, even if FileTransitionLog syncs appends.
            self.log_store.flush_log()?;
            self.committed.insert(id);
        }
        let pending = self.current.take().expect("transaction is still open");
        if let Err(error) = self.finish_commit_checkpoint(&pending.transitions) {
            self.current = Some(pending);
            return Err(error);
        }
        if self.trace {
            crate::diagnostics::stderr!(
                "[Transactions] Committed tx {:032x} (mode={:?}, transitions={}).",
                pending.id,
                self.commit_mode,
                pending.transitions.len()
            );
        }
        let _ = pending.auto_commit;
        let _ = pending.started_ms;
        Ok(())
    }

    fn finish_commit_checkpoint(
        &mut self,
        transitions: &[JournalRecord<T>],
    ) -> Result<(), LinkError> {
        // Never persist an applied marker before its data is durable.
        self.inner.flush()?;
        for transition in transitions {
            self.mark_applied(transition)?;
        }
        self.log_store.flush_log()?;
        self.enforce_retention()
    }
}
