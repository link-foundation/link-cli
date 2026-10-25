//! Exclusive, unlogged replay owned by the version decorator.
use super::{
    GenericTransactionsDecorator, JournalRecord, LinkError, LinksStorage, TransitionLogStore,
};
use doublets::data::LinkReference;

impl<T, S, L> GenericTransactionsDecorator<T, S, L>
where
    T: LinkReference,
    S: LinksStorage<T>,
    L: TransitionLogStore,
{
    pub(super) fn ensure_no_version_replay(&self) -> Result<(), LinkError> {
        if self.version_replay {
            return Err(LinkError::Transaction(
                "Complete version movement before accessing transaction writes or persistence."
                    .into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn begin_version_replay(&mut self) -> Result<(), LinkError> {
        self.ensure_recovery_complete()?;
        if self.current.is_some() {
            return Err(LinkError::Transaction(
                "A transaction still owns the data store.".into(),
            ));
        }
        self.version_replay = true;
        // Rust commits checkpoint synchronously in both modes. Flush again
        // before a durable movement can rely on those applied markers.
        self.flush_version_replay()
    }

    pub(crate) fn replay_version_transition(
        &mut self,
        transition: &JournalRecord<T>,
        undo: bool,
    ) -> Result<(), LinkError> {
        self.require_version_replay()?;
        self.replaying = true;
        let result = if undo {
            self.try_revert_record(transition)
        } else {
            self.try_apply_record(transition, false)
        };
        self.replaying = false;
        result
    }

    pub(crate) fn flush_version_replay(&mut self) -> Result<(), LinkError> {
        self.require_version_replay()?;
        self.inner.flush()?;
        self.log_store.flush_log()
    }

    pub(crate) fn finish_version_replay(&mut self) {
        self.version_replay = false;
    }

    fn require_version_replay(&self) -> Result<(), LinkError> {
        if !self.version_replay {
            return Err(LinkError::Transaction(
                "Version movement does not own replay.".into(),
            ));
        }
        Ok(())
    }
}
