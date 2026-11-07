//! Checkpoint construction and optional publication share the commit mutex.
use super::{
    changes, error, journal, Engine, LinkReference, Rows, Snapshot, State, StateStructure,
};
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
fn empty<T: LinkReference>(structure: StateStructure) -> Snapshot<T> {
    Snapshot {
        revision: 0,
        rows: std::sync::Arc::new(Rows::new(structure)),
    }
}
/// One record of a journal, after the state its predecessors left. Only a first
/// record may be a checkpoint.
fn replay_record<T: LinkReference>(
    index: usize,
    record: &str,
    last: &Snapshot<T>,
    last_next: u128,
    structure: StateStructure,
) -> Result<(Snapshot<T>, u128), LinkError> {
    if index == 0 && record.starts_with("engine3|") {
        journal::decode_checkpoint(record, structure)
    } else {
        journal::decode_after(record, last, last_next, structure)
    }
}
/// Copy an archived journal to an empty log, stopping at one revision, and return
/// the revision the copy ends at. `None` copies the whole archive. Opening the
/// destination gives the engine as it was at that revision, on a new timeline.
///
/// Records are copied unchanged, so an incremental chain restores to any revision
/// it holds, and so does a live journal. Records after the selected revision are
/// not decoded, so a copy can end just before damage. Everything up to it is decoded
/// before the first write, so a refused restore leaves the destination unchanged.
/// After a later failure the destination may hold a shorter copy, so discard it.
/// See `docs/ENGINE-BACKUP.md`.
pub fn restore_backup<T: LinkReference, S: TransitionLogStore, D: TransitionLogStore>(
    source: &mut S,
    destination: &mut D,
    revision: Option<u64>,
) -> Result<u64, LinkError> {
    if !destination.read_log_entries()?.is_empty() {
        return Err(error("restore destination must be empty"));
    }
    let mut entries = source.read_log_entries()?;
    let structure = StateStructure::Flat;
    let (mut last, mut last_next) = (empty::<T>(structure), 1);
    // A journal without a checkpoint starts from the empty revision 0.
    let mut reached = revision == Some(0)
        && !entries
            .first()
            .is_some_and(|record| record.starts_with("engine3|"));
    let mut kept = 0;
    for (index, record) in entries.iter().enumerate() {
        if reached {
            break;
        }
        (last, last_next) = replay_record(index, record, &last, last_next, structure)?;
        kept = index + 1;
        reached = Some(last.revision) == revision;
    }
    if revision.is_some() && !reached {
        return Err(error("revision is not in the backup"));
    }
    entries.truncate(kept);
    for entry in &entries {
        destination.append_log_entry(entry)?;
    }
    destination.flush_log()?;
    if destination.read_log_entries()? != entries {
        return Err(error(
            "restore verification failed: destination differs from the copied records",
        ));
    }
    Ok(last.revision)
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
    /// Bring an earlier backup of this engine up to the current revision by
    /// appending only the revisions it lacks, each as the rows that changed, and
    /// return the revision it now ends at. An empty destination gets a full copy.
    ///
    /// The destination's last revision must still be retained here and hold the
    /// same rows, otherwise it is refused and left alone: take a full backup. The
    /// commit mutex is held only to select the snapshots. After a failure the
    /// destination may end at an earlier revision, and calling again continues
    /// from there. See `docs/ENGINE-BACKUP.md`.
    pub fn backup_incremental_to<D: TransitionLogStore>(
        &self,
        destination: &mut D,
    ) -> Result<u64, LinkError> {
        let existing = destination.read_log_entries()?;
        if existing.is_empty() {
            return self.backup_to(destination, None);
        }
        let structure = self.lock()?.options.state_structure;
        let (mut last, mut last_next) = (empty::<T>(structure), 1);
        for (index, record) in existing.iter().enumerate() {
            (last, last_next) = replay_record(index, record, &last, last_next, structure)?;
        }
        let chain: Vec<(Snapshot<T>, u128)> = {
            let state = self.lock()?;
            if state.history.is_empty() {
                (last.revision == state.current.revision)
                    .then(|| (state.current.clone(), state.committed_next))
                    .into_iter()
                    .collect()
            } else {
                state
                    .history
                    .range(last.revision..)
                    .map(|(_, entry)| entry.clone())
                    .collect()
            }
        };
        let Some((same, same_next)) = chain.first().filter(|(s, _)| s.revision == last.revision)
        else {
            return Err(error("revision is not retained"));
        };
        if same.rows != last.rows || *same_next != last_next {
            return Err(error("backup destination is not a copy of this engine"));
        }
        let entries: Vec<String> = chain
            .windows(2)
            .map(|pair| {
                let (previous, (snapshot, next)) = (&pair[0].0, &pair[1]);
                let ids: Vec<T> = previous
                    .rows
                    .keys()
                    .chain(snapshot.rows.keys())
                    .copied()
                    .collect();
                changes::encode(snapshot, *next, previous, &ids)
            })
            .collect();
        for entry in &entries {
            destination.append_log_entry(entry)?;
        }
        destination.flush_log()?;
        if destination.read_log_entries()? != [existing, entries].concat() {
            return Err(error(
                "backup verification failed: destination differs from the copied records",
            ));
        }
        Ok(chain.last().map_or(last.revision, |(s, _)| s.revision))
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
