//! Checkpoint construction and optional publication share the commit mutex.
use super::{error, journal, Engine, LinkReference, State};
use crate::{LinkError, ReplaceableTransitionLogStore, TransitionLogStore};

/// The records of one retained revision, for a copy that ends at that revision.
fn retained_records<T: LinkReference, L>(
    state: &State<T, L>,
    revision: u64,
) -> Result<Vec<String>, LinkError> {
    if revision != state.current.revision && !state.history.contains_key(&revision) {
        return Err(error("revision is not retained"));
    }
    Ok(records_through(state, revision))
}
fn records<T: LinkReference, L>(state: &State<T, L>) -> Vec<String> {
    records_through(state, state.current.revision)
}
fn records_through<T: LinkReference, L>(state: &State<T, L>, revision: u64) -> Vec<String> {
    let snapshots: Vec<_> = if state.history.is_empty() {
        vec![(&state.current, state.committed_next)]
    } else {
        state
            .history
            .range(..=revision)
            .map(|(_, (snapshot, next))| (snapshot, *next))
            .collect()
    };
    snapshots
        .into_iter()
        .enumerate()
        .map(|(index, (snapshot, next))| {
            // Saved branch descriptors require exact committed metadata. Private
            // reservations remain reserved in this owner but never enter the log.
            journal::encode_format(
                snapshot,
                next,
                if index == 0 { "engine3" } else { "engine1" },
            )
        })
        .collect()
}
impl<T: LinkReference, L: TransitionLogStore> Engine<T, L> {
    /// Export retained committed snapshots without rewriting the live log.
    /// The caller owns atomic publication and synchronization of these records.
    pub fn export_checkpoint(&self) -> Result<Vec<String>, LinkError> {
        let state = self.lock()?;
        Ok(records(&state))
    }
    /// Export the retained snapshots up to one retained revision, as a journal
    /// whose reopened head is that revision.
    pub fn export_checkpoint_at(&self, revision: u64) -> Result<Vec<String>, LinkError> {
        retained_records(&*self.lock()?, revision)
    }
    /// Copy a consistent cut to an empty log while commits continue, and return
    /// the revision it ends at. `None` selects the current revision; a retained
    /// earlier revision gives a point-in-time copy.
    ///
    /// The commit mutex is held only while the cut is encoded, never during
    /// destination I/O, so a slow or failing destination cannot delay or fence
    /// this engine. After a failure the destination may hold a shorter copy, so
    /// discard it. See `docs/ENGINE-BACKUP.md`.
    pub fn backup_to<D: TransitionLogStore>(
        &self,
        destination: &mut D,
        revision: Option<u64>,
    ) -> Result<u64, LinkError> {
        if !destination.read_log_entries()?.is_empty() {
            return Err(error("backup destination must be empty"));
        }
        let (revision, entries) = {
            let state = self.lock()?;
            let revision = revision.unwrap_or(state.current.revision);
            (revision, retained_records(&state, revision)?)
        };
        for entry in &entries {
            destination.append_log_entry(entry)?;
        }
        destination.flush_log()?;
        if destination.read_log_entries()? != entries {
            return Err(error(
                "backup verification failed: destination differs from the copied records",
            ));
        }
        Ok(revision)
    }
}
impl<T: LinkReference, L: ReplaceableTransitionLogStore> Engine<T, L> {
    /// Atomically replace the journal while holding the commit mutex. Identical
    /// records return false. Any replacement error fences this owner until reopen.
    /// Unsupported capability leaves the engine available for ordinary commits.
    pub fn compact_journal(&self) -> Result<bool, LinkError> {
        let mut state = self.lock()?;
        if !state.log.supports_replacement() {
            return Err(error("selected log does not support journal replacement"));
        }
        let entries = records(&state);
        match state.log.replace_log_entries(&entries) {
            Ok(changed) => Ok(changed),
            Err(cause) => {
                state.poisoned = true;
                Err(cause)
            }
        }
    }
}
