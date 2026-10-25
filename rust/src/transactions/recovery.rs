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

use super::journal_validation::RecoveryJournal;
use super::log::TransitionLogStore;
use super::types::{
    GenericTransition, LogRetentionPolicy, TransitionKind, APPLIED_MARKER_PREFIX,
    ROLLBACK_MARKER_PREFIX,
};
use super::{new_transaction_id, now_unix_ms, GenericTransactionsDecorator, JournalRecord};

impl<T, S, L> GenericTransactionsDecorator<T, S, L>
where
    T: LinkReference,
    S: LinksStorage<T>,
    L: TransitionLogStore,
{
    /// Public helper for higher-level decorators (e.g. version control)
    /// — applies a single transition without writing a new log entry.
    /// Rejects replay while a transaction or its cleanup still owns the data.
    pub fn apply_transition(&mut self, transition: &GenericTransition<T>) -> Result<(), LinkError> {
        self.ensure_replay_allowed()?;
        self.replaying = true;
        let result = self.try_apply_transition(transition, false);
        self.replaying = false;
        result
    }

    /// Reverts one transition, reporting storage errors to the caller.
    /// Rejects replay while a transaction or its cleanup still owns the data.
    pub fn revert_transition(
        &mut self,
        transition: &GenericTransition<T>,
    ) -> Result<(), LinkError> {
        self.ensure_replay_allowed()?;
        self.replaying = true;
        let result = self.try_revert_transition(transition);
        self.replaying = false;
        result
    }

    /// Replays a complete record, including names, without logging another write.
    pub fn apply_record(&mut self, record: &JournalRecord<T>) -> Result<(), LinkError> {
        self.ensure_replay_allowed()?;
        self.validate_record_backend(record)?;
        self.replaying = true;
        let result = self.try_apply_record(record, false);
        self.replaying = false;
        result
    }

    /// Undoes a complete record, including names, without logging another write.
    pub fn revert_record(&mut self, record: &JournalRecord<T>) -> Result<(), LinkError> {
        self.ensure_replay_allowed()?;
        self.validate_record_backend(record)?;
        self.replaying = true;
        let result = self.try_revert_record(record);
        self.replaying = false;
        result
    }

    fn validate_record_backend(&mut self, record: &JournalRecord<T>) -> Result<(), LinkError> {
        record.validate()?;
        if !record.names.is_empty() && self.inner.capture_names()?.is_none() {
            return Err(LinkError::StorageError(
                "journal requires a named storage adapter".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn try_apply_record(
        &mut self,
        record: &JournalRecord<T>,
        record_applied: bool,
    ) -> Result<(), LinkError> {
        if !record.name_only {
            self.try_apply_transition(&record.transition, false)?;
        }
        self.replay_names(record, false)?;
        if record_applied {
            self.inner.flush()?;
            self.mark_applied(record)?;
            self.log_store.flush_log()?;
        }
        Ok(())
    }

    pub(super) fn try_revert_record(&mut self, record: &JournalRecord<T>) -> Result<(), LinkError> {
        if !record.name_only {
            self.try_revert_transition(&record.transition)?;
        }
        self.replay_names(record, true)
    }

    fn ensure_replay_allowed(&self) -> Result<(), LinkError> {
        if self.has_open_transaction() {
            return Err(LinkError::Transaction(
                "Complete the current transaction or its cleanup before unlogged replay.".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn try_apply_transition(
        &mut self,
        transition: &GenericTransition<T>,
        record_applied: bool,
    ) -> Result<(), LinkError> {
        let zero = T::from_byte(0);
        let result: Result<(), LinkError> = match transition.kind {
            TransitionKind::Create => {
                if transition.after.index != zero {
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
        result?;
        if record_applied {
            self.inner.flush()?;
            self.mark_applied(transition)?;
            self.log_store.flush_log()?;
        }
        Ok(())
    }

    pub(super) fn try_revert_transition(
        &mut self,
        transition: &GenericTransition<T>,
    ) -> Result<(), LinkError> {
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
                if transition.before.index != zero {
                    // Restoring a higher mapped address can allocate this
                    // address as an empty intermediate slot. Existence alone
                    // does not mean its original contents were restored.
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
        result
    }

    pub(super) fn mark_applied(
        &mut self,
        transition: &GenericTransition<T>,
    ) -> Result<(), LinkError> {
        if !self.applied.contains(&transition.sequence) {
            self.write_marker(&format!("{APPLIED_MARKER_PREFIX}{}", transition.sequence))?;
            self.applied.insert(transition.sequence);
            if transition.sequence > self.applied_sequence {
                self.applied_sequence = transition.sequence;
            }
        }
        Ok(())
    }

    // ----- Recovery -------------------------------------------------------

    pub(super) fn ensure_recovery_complete(&self) -> Result<(), LinkError> {
        if self.recovery_pending {
            return Err(LinkError::Transaction(
                "Retry recovery before further transaction writes or replay.".into(),
            ));
        }
        Ok(())
    }

    /// Validates the sidecar journal before rebuilding history and replaying it.
    /// Unrelated names are ignored; malformed or contradictory transaction
    /// records fail before rows or live indexes change. Identical retries are
    /// collapsed. A failure retains write admission until recovery succeeds.
    pub fn recover(&mut self) -> Result<(), LinkError> {
        if self.current.is_some() || self.version_replay {
            return Err(LinkError::Transaction(
                "Cannot recover while a transaction is open.".into(),
            ));
        }
        self.recovery_pending = true;
        match self.recover_journal() {
            Ok(()) => {
                self.recovery_pending = false;
                Ok(())
            }
            Err(error) => {
                if self.trace {
                    crate::diagnostics::stderr!(
                        "[Transactions] Recovery failed; admission retained: {error}."
                    );
                }
                Err(error)
            }
        }
    }

    fn recover_journal(&mut self) -> Result<(), LinkError> {
        let journal = RecoveryJournal::<T>::parse(self.log_store.read_log_entries()?)?;
        // Parsing already validates every payload. Probe optional adapter
        // capability once, avoiding a full sidecar scan per retained record.
        if let Some(record) = journal.log.iter().find(|record| !record.names.is_empty()) {
            self.validate_record_backend(record)?;
        }
        if journal.images.iter().any(|image| image.names.is_some())
            && self.inner.capture_names()?.is_none()
        {
            return Err(LinkError::StorageError(
                "undo image requires a named storage adapter".into(),
            ));
        }
        if self.reload_pending {
            self.inner.reload()?;
            self.reload_pending = false;
        }
        self.log = journal.log;
        self.committed = journal.committed;
        self.rolled_back = journal.rolled_back;
        self.applied = journal.applied;
        self.sequence_counter = journal.sequence;
        self.applied_sequence = journal.applied_sequence;

        // An older failed checkpoint can precede a later applied commit. Redo
        // the whole committed suffix in order, including already-applied rows.
        let log_snapshot: Vec<JournalRecord<T>> = self.log.clone();
        let mut replay_from = log_snapshot
            .iter()
            .position(|transition| {
                self.committed.contains(&transition.transaction_id)
                    && !self.applied.contains(&transition.sequence)
            })
            .unwrap_or(log_snapshot.len());
        let pending_images = journal
            .images
            .into_iter()
            .filter(|image| {
                !self.committed.contains(&image.owner) && !self.rolled_back.contains(&image.owner)
            })
            .collect::<Vec<_>>();
        if let Some(base) = pending_images.iter().map(|image| image.base).min() {
            replay_from =
                replay_from.min(log_snapshot.partition_point(|record| record.sequence <= base));
        }
        self.replaying = true;
        let result = (|| -> Result<Vec<u128>, LinkError> {
            let mut pending_tx_ids = Vec::new();
            let mut pending_seen = std::collections::HashSet::new();
            let imaged = pending_images
                .iter()
                .map(|image| image.owner)
                .collect::<std::collections::HashSet<_>>();
            if !pending_images.is_empty() {
                // Undo newest effects first, interleaving legacy records and
                // complete images by their base sequence. The parser rejects
                // ambiguous equal-base owners, independent of physical order.
                let mut images = pending_images.iter().collect::<Vec<_>>();
                images.sort_by_key(|image| image.base);
                let mut records = log_snapshot
                    .iter()
                    .rev()
                    .filter(|record| {
                        !self.committed.contains(&record.transaction_id)
                            && !self.rolled_back.contains(&record.transaction_id)
                            && !imaged.contains(&record.transaction_id)
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .peekable();
                for image in images.into_iter().rev() {
                    while records
                        .peek()
                        .is_some_and(|record| record.sequence > image.base)
                    {
                        let record = records.next().expect("peeked record");
                        self.try_revert_record(record)?;
                        if pending_seen.insert(record.transaction_id) {
                            pending_tx_ids.push(record.transaction_id);
                        }
                    }
                    self.restore_image(image)?;
                    if pending_seen.insert(image.owner) {
                        pending_tx_ids.push(image.owner);
                    }
                }
                for record in records {
                    self.try_revert_record(record)?;
                    if pending_seen.insert(record.transaction_id) {
                        pending_tx_ids.push(record.transaction_id);
                    }
                }
            }
            for transition in &log_snapshot[replay_from..] {
                if !self.committed.contains(&transition.transaction_id) {
                    continue;
                }
                self.try_apply_record(transition, false)?;
            }
            // Auto-rollback transitions written but never committed and never rolled back (R10).
            for transition in log_snapshot
                .iter()
                .rev()
                .filter(|_| pending_images.is_empty())
            {
                if self.committed.contains(&transition.transaction_id) {
                    continue;
                }
                if self.rolled_back.contains(&transition.transaction_id) {
                    continue;
                }
                self.try_revert_record(transition)?;
                if pending_seen.insert(transition.transaction_id) {
                    pending_tx_ids.push(transition.transaction_id);
                }
            }
            Ok(pending_tx_ids)
        })();
        self.replaying = false;
        let pending_tx_ids = result?;
        self.inner.flush()?;
        // The gap must remain a restart anchor until the complete final state
        // (including undo) is durable, even if a later marker append fails.
        for transition in &log_snapshot[replay_from..] {
            if self.committed.contains(&transition.transaction_id) {
                self.mark_applied(transition)?;
            }
        }
        for tx_id in pending_tx_ids {
            self.rolled_back.insert(tx_id);
            self.write_marker(&format!("{ROLLBACK_MARKER_PREFIX}{tx_id:032x}"))?;
        }
        self.log_store.flush_log()?;
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
            let head = self.log[0].clone();
            if !self.rolled_back.contains(&head.transaction_id)
                && !self.applied.contains(&head.sequence)
            {
                if !self.committed.contains(&head.transaction_id) {
                    break; // A pending transaction is not reclaimable.
                }
                self.replaying = true;
                let result = self.try_apply_record(&head, true);
                self.replaying = false;
                result?;
                if !self.applied.contains(&head.sequence) {
                    break; // R7: never drop an un-applied transition.
                }
            }
            self.log.remove(0);
            if self.trace {
                crate::diagnostics::stderr!(
                    "[Transactions] Dropped completed transition seq={} per sized retention.",
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
        let chunk: Vec<JournalRecord<T>> =
            self.log.iter().take(chunk_size as usize).cloned().collect();
        // Rollback has already durably undone these writes. Reapplying them
        // would resurrect aborted data; archives contain committed changes only.
        let archived: Vec<_> = chunk
            .iter()
            .filter(|transition| !self.rolled_back.contains(&transition.transaction_id))
            .cloned()
            .collect();
        for transition in &archived {
            if !self.applied.contains(&transition.sequence) {
                if !self.committed.contains(&transition.transaction_id) {
                    return Ok(()); // Never apply or discard a pending chunk.
                }
                self.replaying = true;
                let result = self.try_apply_record(transition, true);
                self.replaying = false;
                result?;
                if !self.applied.contains(&transition.sequence) {
                    return Ok(()); // never drop un-applied
                }
            }
        }
        if archived.is_empty() {
            self.log.drain(0..chunk.len());
            return Ok(());
        }
        std::fs::create_dir_all(archive_directory).map_err(|error| {
            LinkError::StorageError(format!(
                "failed to create archive dir {}: {error}",
                archive_directory.display()
            ))
        })?;
        let timestamp = now_unix_ms();
        // Independent journals may share this directory and restart their
        // counters at the same realtime. Claim the path in the open itself;
        // an existence check followed by create could still replace a writer.
        let (mut file, path) = loop {
            let file_name = format!(
                "transitions-chunk-{timestamp}-{:032x}.log",
                new_transaction_id()
            );
            let path = archive_directory.join(file_name);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => break (file, path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if self.trace {
                        crate::diagnostics::stderr!(
                            "[Transactions] Archive entry already owned: {}.",
                            path.display()
                        );
                    }
                }
                Err(error) => {
                    return Err(LinkError::StorageError(format!(
                        "failed to create archive file {}: {error}",
                        path.display()
                    )))
                }
            }
        };
        use std::io::Write;
        for transition in &archived {
            writeln!(file, "{}", transition.serialize())?;
        }
        file.flush()?;
        if self.trace {
            crate::diagnostics::stderr!(
                "[Transactions] Archived {} transitions to {}.",
                archived.len(),
                path.display()
            );
        }
        self.log.drain(0..chunk.len());
        Ok(())
    }
}
