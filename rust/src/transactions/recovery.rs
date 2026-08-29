//! Transition replay, crash recovery and log retention for
//! [`GenericTransactionsDecorator`].
//!
//! Extracted from `transactions/mod.rs` for issue #100: the file had grown
//! past the 1000-line limit enforced by `rust/scripts/check-file-size.rs`.
//! These are the paths that read the sidecar log back — applying and
//! reverting individual transitions, replaying an interrupted run, and
//! trimming or archiving the log once it grows — as opposed to the write
//! paths that record new transitions in the parent module.

use std::path::Path;

use doublets::data::LinkReference;

use crate::error::LinkError;
use crate::storage::LinksStorage;

use super::log::TransitionLogStore;
use super::types::{
    GenericTransition, LogRetentionPolicy, TransitionKind, APPLIED_MARKER_PREFIX,
    COMMIT_MARKER_PREFIX, ROLLBACK_MARKER_PREFIX, TRANSITION_NAME_PREFIX,
};
use super::{insert_ordered, new_transaction_id, now_unix_ms, GenericTransactionsDecorator};

impl<T, S, L> GenericTransactionsDecorator<T, S, L>
where
    T: LinkReference,
    S: LinksStorage<T>,
    L: TransitionLogStore,
{
    /// Public helper for higher-level decorators (e.g. version control)
    /// — applies a single transition without writing a new log entry.
    pub fn apply_transition(&mut self, transition: &GenericTransition<T>) {
        self.replaying = true;
        self.try_apply_transition(transition, false);
        self.replaying = false;
    }

    /// Public helper for higher-level decorators (e.g. version control)
    /// — reverts a single transition without writing a new log entry.
    pub fn revert_transition(&mut self, transition: &GenericTransition<T>) {
        self.replaying = true;
        self.try_revert_transition(transition);
        self.replaying = false;
    }

    pub(super) fn try_apply_transition(
        &mut self,
        transition: &GenericTransition<T>,
        record_applied: bool,
    ) {
        let zero = T::from_byte(0);
        let result: Result<(), LinkError> = match transition.kind {
            TransitionKind::Create => {
                if transition.after.index != zero && !self.inner.link_exists(transition.after.index)
                {
                    self.inner
                        .ensure_link_created(transition.after.index)
                        .and_then(|_| {
                            self.inner
                                .update_link(
                                    transition.after.index,
                                    transition.after.source,
                                    transition.after.target,
                                )
                                .map(|_| ())
                        })
                } else {
                    Ok(())
                }
            }
            TransitionKind::Update => {
                if transition.after.index != zero && self.inner.link_exists(transition.after.index)
                {
                    self.inner
                        .update_link(
                            transition.after.index,
                            transition.after.source,
                            transition.after.target,
                        )
                        .map(|_| ())
                } else {
                    Ok(())
                }
            }
            TransitionKind::Delete => {
                if transition.before.index != zero
                    && self.inner.link_exists(transition.before.index)
                {
                    self.inner.delete_link(transition.before.index).map(|_| ())
                } else {
                    Ok(())
                }
            }
        };
        if let Err(e) = result {
            if self.trace {
                eprintln!(
                    "[Transactions] Failed to apply transition seq={}: {e}",
                    transition.sequence
                );
            }
        }
        if record_applied {
            let _ = self.mark_applied(transition);
        }
    }

    pub(super) fn try_revert_transition(&mut self, transition: &GenericTransition<T>) {
        let zero = T::from_byte(0);
        let result: Result<(), LinkError> = match transition.kind {
            TransitionKind::Create => {
                if transition.after.index != zero && self.inner.link_exists(transition.after.index)
                {
                    self.inner.delete_link(transition.after.index).map(|_| ())
                } else {
                    Ok(())
                }
            }
            TransitionKind::Update => {
                if transition.before.index != zero
                    && self.inner.link_exists(transition.before.index)
                {
                    self.inner
                        .update_link(
                            transition.before.index,
                            transition.before.source,
                            transition.before.target,
                        )
                        .map(|_| ())
                } else {
                    Ok(())
                }
            }
            TransitionKind::Delete => {
                if transition.before.index != zero
                    && !self.inner.link_exists(transition.before.index)
                {
                    self.inner
                        .ensure_link_created(transition.before.index)
                        .and_then(|_| {
                            self.inner
                                .update_link(
                                    transition.before.index,
                                    transition.before.source,
                                    transition.before.target,
                                )
                                .map(|_| ())
                        })
                } else {
                    Ok(())
                }
            }
        };
        if let Err(e) = result {
            if self.trace {
                eprintln!(
                    "[Transactions] Failed to revert transition seq={}: {e}",
                    transition.sequence
                );
            }
        }
    }

    pub(super) fn mark_applied(
        &mut self,
        transition: &GenericTransition<T>,
    ) -> Result<(), LinkError> {
        if self.applied.insert(transition.sequence) {
            self.write_marker(&format!("{APPLIED_MARKER_PREFIX}{}", transition.sequence))?;
            if transition.sequence > self.applied_sequence {
                self.applied_sequence = transition.sequence;
            }
        }
        Ok(())
    }

    // ----- Recovery -------------------------------------------------------

    /// Rebuilds the in-memory log and marker tables from the sidecar
    /// log store and re-applies committed-but-unapplied side-effects.
    ///
    /// Entries that cannot be parsed are skipped: an append-only log
    /// can end in the partial entry of a crashed write, and the
    /// links-backed log can hold names that belong to other features.
    /// An entry whose addresses do not fit into `T` is *not* skipped —
    /// that means the log was written by a wider address type and
    /// silently dropping it would corrupt the recovered state.
    pub fn recover(&mut self) -> Result<(), LinkError> {
        self.log.clear();
        self.committed.clear();
        self.rolled_back.clear();
        self.applied.clear();
        self.sequence_counter = 0;
        self.applied_sequence = 0;

        for entry in self.log_store.read_log_entries()? {
            if let Some(payload) = entry.strip_prefix(TRANSITION_NAME_PREFIX) {
                match GenericTransition::<T>::parse(payload) {
                    Ok(transition) => {
                        insert_ordered(&mut self.log, transition);
                        if transition.sequence > self.sequence_counter {
                            self.sequence_counter = transition.sequence;
                        }
                    }
                    Err(LinkError::AddressOutOfRange(value)) => {
                        return Err(LinkError::AddressOutOfRange(value))
                    }
                    Err(error) => {
                        if self.trace {
                            eprintln!("[Transactions] Skipping unreadable log entry: {error}");
                        }
                    }
                }
            } else if let Some(rest) = entry.strip_prefix(COMMIT_MARKER_PREFIX) {
                if let Ok(tx_id) = u128::from_str_radix(rest, 16) {
                    self.committed.insert(tx_id);
                }
            } else if let Some(rest) = entry.strip_prefix(ROLLBACK_MARKER_PREFIX) {
                if let Ok(tx_id) = u128::from_str_radix(rest, 16) {
                    self.rolled_back.insert(tx_id);
                }
            } else if let Some(rest) = entry.strip_prefix(APPLIED_MARKER_PREFIX) {
                if let Ok(seq) = rest.parse::<i64>() {
                    self.applied.insert(seq);
                    if seq > self.applied_sequence {
                        self.applied_sequence = seq;
                    }
                }
            }
        }

        // Re-apply committed-but-not-applied transitions (crash mid-async).
        let log_snapshot: Vec<GenericTransition<T>> = self.log.clone();
        self.replaying = true;
        for transition in &log_snapshot {
            if !self.committed.contains(&transition.transaction_id) {
                continue;
            }
            if self.applied.contains(&transition.sequence) {
                continue;
            }
            self.try_apply_transition(transition, true);
        }
        // Auto-rollback transitions written but never committed and never rolled back (R10).
        let mut pending_tx_ids: Vec<u128> = Vec::new();
        for transition in log_snapshot.iter().rev() {
            if self.committed.contains(&transition.transaction_id) {
                continue;
            }
            if self.rolled_back.contains(&transition.transaction_id) {
                continue;
            }
            self.try_revert_transition(transition);
            if !pending_tx_ids.contains(&transition.transaction_id) {
                pending_tx_ids.push(transition.transaction_id);
            }
        }
        self.replaying = false;
        for tx_id in pending_tx_ids {
            self.rolled_back.insert(tx_id);
            self.write_marker(&format!("{ROLLBACK_MARKER_PREFIX}{tx_id:032x}"))?;
        }
        Ok(())
    }

    pub(super) fn enforce_retention(&mut self) -> Result<(), LinkError> {
        match self.retention_policy.clone() {
            LogRetentionPolicy::Infinite => Ok(()),
            LogRetentionPolicy::Sized { max_transitions } => self.enforce_sized(max_transitions),
            LogRetentionPolicy::Chunked {
                chunk_size,
                archive_directory,
            } => self.enforce_chunked(chunk_size, &archive_directory),
        }
    }

    fn enforce_sized(&mut self, max_transitions: u64) -> Result<(), LinkError> {
        if max_transitions == 0 {
            return Ok(());
        }
        while self.log.len() as u64 > max_transitions {
            let head = self.log[0];
            if !self.applied.contains(&head.sequence) {
                self.replaying = true;
                self.try_apply_transition(&head, true);
                self.replaying = false;
                if !self.applied.contains(&head.sequence) {
                    break; // R7: never drop an un-applied transition.
                }
            }
            self.log.remove(0);
            if self.trace {
                eprintln!(
                    "[Transactions] Dropped applied transition seq={} per sized retention.",
                    head.sequence
                );
            }
        }
        Ok(())
    }

    fn enforce_chunked(
        &mut self,
        chunk_size: u64,
        archive_directory: &Path,
    ) -> Result<(), LinkError> {
        if chunk_size == 0 {
            return Ok(());
        }
        if (self.log.len() as u64) < chunk_size {
            return Ok(());
        }
        let chunk: Vec<GenericTransition<T>> =
            self.log.iter().take(chunk_size as usize).copied().collect();
        for transition in &chunk {
            if !self.applied.contains(&transition.sequence) {
                self.replaying = true;
                self.try_apply_transition(transition, true);
                self.replaying = false;
                if !self.applied.contains(&transition.sequence) {
                    return Ok(()); // never drop un-applied
                }
            }
        }
        std::fs::create_dir_all(archive_directory).map_err(|error| {
            LinkError::StorageError(format!(
                "failed to create archive dir {}: {error}",
                archive_directory.display()
            ))
        })?;
        let timestamp = now_unix_ms();
        let file_name = format!(
            "transitions-chunk-{timestamp}-{:032x}.log",
            new_transaction_id()
        );
        let path = archive_directory.join(file_name);
        use std::io::Write;
        let mut file = std::fs::File::create(&path).map_err(|error| {
            LinkError::StorageError(format!(
                "failed to create archive file {}: {error}",
                path.display()
            ))
        })?;
        for transition in &chunk {
            writeln!(file, "{}", transition.serialize())?;
        }
        file.flush()?;
        if self.trace {
            eprintln!(
                "[Transactions] Archived {} transitions to {}.",
                chunk.len(),
                path.display()
            );
        }
        self.log.drain(0..chunk.len());
        Ok(())
    }
}
