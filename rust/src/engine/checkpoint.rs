//! Checkpoint construction and optional publication share the commit mutex.
use super::{error, journal, Engine, LinkReference, State};
use crate::{LinkError, ReplaceableTransitionLogStore, TransitionLogStore};

fn records<T: LinkReference, L>(state: &State<T, L>) -> Vec<String> {
    let snapshots: Vec<_> = if state.history.is_empty() {
        vec![(&state.current, state.committed_next)]
    } else {
        state
            .history
            .values()
            .map(|(snapshot, next)| (snapshot, *next))
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
