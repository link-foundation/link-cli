//! Bounds the durable journal by the retention policy (issue #195).
//!
//! Retention trims the in-memory history after each commit or rollback.
//! Without compaction the journal on disk kept every entry ever written, so
//! each process start re-read the complete history and every write grew a
//! file that no policy could shrink. When the log supports it, the journal
//! is rewritten to the entries that still matter once it has grown to
//! about twice that size, so the rewrite is amortized over many appends.
//!
//! A compacted journal keeps, in order:
//!
//! 1. a `__transactions:sequence:<n>` watermark, so sequence numbers (which
//!    version-control tags and branches reference) are never reused;
//! 2. an applied marker for the highest applied sequence;
//! 3. every retained record, unchanged;
//! 4. the commit or rollback decision of every retained record's owner and
//!    the newest decision, which version-control publication recovery reads;
//! 5. the applied markers of retained records.
//!
//! Undo images belong to undecided owners only while a transaction is open
//! or recovery is pending; compaction never runs then, so decided images are
//! dropped. A journal with any undecided record is left untouched.

use std::collections::HashSet;

use doublets::data::LinkReference;

use super::types::{
    LogRetentionPolicy, APPLIED_MARKER_PREFIX, COMMIT_MARKER_PREFIX, ROLLBACK_MARKER_PREFIX,
    SEQUENCE_MARKER_PREFIX, TRANSITION_NAME_PREFIX,
};
use super::{GenericTransactionsDecorator, LinkError, LinksStorage, TransitionLogStore};

/// Entries a journal may grow by beyond twice its live size before it is
/// compacted, so small retention windows do not rewrite on every commit.
const COMPACTION_SLACK: usize = 256;

impl<T, S, L> GenericTransactionsDecorator<T, S, L>
where
    T: LinkReference,
    S: LinksStorage<T>,
    L: TransitionLogStore,
{
    /// Appends one journal entry, counting it towards the compaction budget.
    pub(super) fn append_journal_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        // Count even a failed append: it may have reached the file.
        self.journal_entries = self.journal_entries.saturating_add(1);
        self.log_store.append_log_entry(entry)
    }

    /// Number of entries the durable journal currently holds.
    pub fn journal_entry_count(&self) -> usize {
        self.journal_entries
    }

    /// Rewrites the durable journal to its retained entries when it has
    /// outgrown them, see the module documentation.
    pub(super) fn compact_journal_if_needed(&mut self) -> Result<(), LinkError> {
        if matches!(self.retention_policy, LogRetentionPolicy::Infinite)
            || !self.log_store.supports_compaction()
            || self.has_open_transaction()
        {
            return Ok(());
        }
        // Each retained record needs at most itself, a decision and an
        // applied marker; the estimate avoids building entries every commit.
        let live = self.log.len().saturating_mul(3).saturating_add(3);
        if self.journal_entries <= live.saturating_mul(2).saturating_add(COMPACTION_SLACK) {
            return Ok(());
        }
        let Some(entries) = self.compacted_journal() else {
            return Ok(());
        };
        // Appends are durable before their replacement is published.
        self.log_store.flush_log()?;
        let before = self.journal_entries;
        self.log_store.compact_log_entries(&entries)?;
        self.journal_entries = entries.len();
        if self.trace {
            crate::diagnostics::stderr!(
                "[Transactions] Compacted journal from {before} to {} entries.",
                entries.len()
            );
        }
        Ok(())
    }

    /// The retained journal, or `None` while any retained record's owner
    /// is still undecided.
    fn compacted_journal(&self) -> Option<Vec<String>> {
        let mut owners = Vec::new();
        let mut seen = HashSet::new();
        for record in &self.log {
            if seen.insert(record.transaction_id) {
                owners.push(record.transaction_id);
            }
        }
        let mut decisions = Vec::with_capacity(owners.len() + 1);
        for owner in owners {
            let prefix = if self.committed.contains(&owner) {
                COMMIT_MARKER_PREFIX
            } else if self.rolled_back.contains(&owner) {
                ROLLBACK_MARKER_PREFIX
            } else {
                return None;
            };
            decisions.push(format!("{prefix}{owner:032x}"));
        }
        if let Some(last) = &self.last_decision {
            if !decisions.contains(last) {
                decisions.push(last.clone());
            }
        }
        let mut entries = Vec::with_capacity(self.log.len() * 2 + decisions.len() + 2);
        if self.sequence_counter > 0 {
            entries.push(format!("{SEQUENCE_MARKER_PREFIX}{}", self.sequence_counter));
        }
        let applied_retained = self
            .log
            .iter()
            .any(|record| record.sequence == self.applied_sequence);
        if self.applied_sequence > 0 && !applied_retained {
            entries.push(format!("{APPLIED_MARKER_PREFIX}{}", self.applied_sequence));
        }
        for record in &self.log {
            entries.push(format!("{TRANSITION_NAME_PREFIX}{}", record.serialize()));
        }
        entries.extend(decisions);
        for record in &self.log {
            if self.applied.contains(&record.sequence) {
                entries.push(format!("{APPLIED_MARKER_PREFIX}{}", record.sequence));
            }
        }
        Some(entries)
    }
}
