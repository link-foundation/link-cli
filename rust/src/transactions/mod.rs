//! Optional transactions layer for the Rust link-cli.
//!
//! Mirrors the C# `TransactionsDecorator` in
//! `csharp/Foundation.Data.Doublets.Cli.Library/TransactionsDecorator.cs`.
//!
//! The decorator records every `create` / `update` / `delete` as a
//! reversible [`GenericTransition`] in a sidecar log. It supports
//! explicit transactions, sync commits, three retention policies, and
//! crash recovery (R1-R7, R10).
//!
//! Optional — when not opted in, the bare
//! [`NamedTypesDecorator`] behaves
//! identically (R8, R9, R17).
//!
//! # Reuse outside the CLI
//!
//! [`GenericTransactionsDecorator`] is generic over three things:
//!
//! * `T` — the doublets address type (`u32`, `u64`, `usize`, ...);
//! * `S` — the wrapped store, any [`LinksStorage<T>`] implementation,
//!   including [`DoubletsStorage`](crate::DoubletsStorage) over a
//!   file-mapped or caller-owned `doublets::unit::Store`;
//! * `L` — the transitions log, any [`TransitionLogStore`].
//!
//! [`TransactionsDecorator`] is the `u32` + `NamedTypesDecorator`
//! specialisation used by `clink` itself.
//!
//! ```no_run
//! use link_cli::transactions::{
//!     CommitMode, FileTransitionLog, GenericTransactionsDecorator, LogRetentionPolicy,
//! };
//! use link_cli::DoubletsStorage;
//!
//! # fn main() -> Result<(), link_cli::LinkError> {
//! let store = DoubletsStorage::<usize, _>::open_exclusive("db.links")?;
//! let log = FileTransitionLog::open("db.transitions.log")?;
//! let mut tx = GenericTransactionsDecorator::new(
//!     store,
//!     log,
//!     LogRetentionPolicy::default(),
//!     CommitMode::default(),
//!     false,
//! )?;
//!
//! tx.begin_transaction()?;
//! let link = tx.create(0, 0)?;
//! tx.commit()?;
//! tx.save()?;
//! # let _ = link;
//! # Ok(())
//! # }
//! ```
//!
//! # Durability
//!
//! Transitions are appended to the log *before* the write they describe
//! is reported as committed, and [`FileTransitionLog`] `fsync`s each
//! append by default, so a crash can lose at most the transition that
//! was in flight. The data store itself is made durable by
//! [`GenericTransactionsDecorator::save`], which calls
//! [`LinksStorage::flush`] — for a memory-mapped store that is the
//! `fsync` of the mapping; for the CLI's in-memory store it is the
//! rewrite of the database file. Recovery, run by
//! [`GenericTransactionsDecorator::new`], replays committed-but-unapplied
//! transitions and rolls back transitions that were never committed, so
//! a store that lost unflushed writes is brought back in line with the
//! log.

mod log;
mod types;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use doublets::data::LinkReference;

use crate::error::LinkError;
use crate::link::GenericLink;
use crate::named_types::NamedTypesDecorator;
use crate::storage::{LinksStorage, LinksStorageRef};

pub use log::{FileTransitionLog, TransitionLogStore};
pub use types::{
    CommitMode, DoubletLink, GenericDoubletLink, GenericTransition, LogRetentionPolicy, Transition,
    TransitionKind,
};
use types::{
    APPLIED_MARKER_PREFIX, COMMIT_MARKER_PREFIX, ROLLBACK_MARKER_PREFIX, TRANSITION_NAME_PREFIX,
};

/// Pending state of a transaction (used by the explicit transaction
/// handle and by per-write auto-transactions).
struct PendingTransaction<T> {
    id: u128,
    transitions: Vec<GenericTransition<T>>,
    auto_commit: bool,
    started_ms: i64,
}

/// One link address and the `(before, after)` states a single logical
/// write left it in, after collapsing repeated callbacks for that address.
type ObservedChange<T> = (T, GenericDoubletLink<T>, GenericDoubletLink<T>);

/// Folds one `(before, after)` callback into `observed`.
///
/// Mirrors the handler `TransactionsDecorator.RunWrite` installs in the C#
/// implementation: repeated callbacks for the same address are collapsed into
/// a single change that keeps the *first* `before` (the state the rollback has
/// to restore) and the *last* `after` (the state the write ended at), and the
/// first-seen order of addresses is preserved so the transitions replay in the
/// order the storage produced them.
fn record_observed<T: LinkReference>(
    observed: &mut Vec<ObservedChange<T>>,
    before: GenericLink<T>,
    after: GenericLink<T>,
) {
    let zero = T::from_byte(0);
    let key = if before.index != zero {
        before.index
    } else {
        after.index
    };
    if key == zero {
        return;
    }
    let before = GenericDoubletLink::from_link(&before);
    let after = GenericDoubletLink::from_link(&after);
    match observed.iter_mut().find(|(index, _, _)| *index == key) {
        Some(entry) => {
            if entry.1.index == zero {
                entry.1 = before;
            }
            entry.2 = after;
        }
        None => observed.push((key, before, after)),
    }
}

/// Snapshot of an open transaction (returned by [`GenericTransactionsDecorator::begin_transaction`]).
#[derive(Debug, Clone)]
pub struct TransactionHandle {
    pub id: u128,
    pub started_ms: i64,
}

/// The transactions decorator wraps any [`LinksStorage`] and records
/// every write as a reversible [`GenericTransition`] in `log_store`.
pub struct GenericTransactionsDecorator<T, S, L>
where
    T: LinkReference,
    S: LinksStorage<T>,
    L: TransitionLogStore,
{
    inner: S,
    log_store: L,
    log: Vec<GenericTransition<T>>,
    committed: HashSet<u128>,
    rolled_back: HashSet<u128>,
    applied: HashSet<i64>,
    current: Option<PendingTransaction<T>>,
    sequence_counter: i64,
    applied_sequence: i64,
    retention_policy: LogRetentionPolicy,
    commit_mode: CommitMode,
    replaying: bool,
    trace: bool,
}

/// The `u32` + [`NamedTypesDecorator`] specialisation used by `clink`.
pub type TransactionsDecorator =
    GenericTransactionsDecorator<u32, NamedTypesDecorator, NamedTypesDecorator>;

impl<T, S, L> GenericTransactionsDecorator<T, S, L>
where
    T: LinkReference,
    S: LinksStorage<T>,
    L: TransitionLogStore,
{
    /// Creates a new transactions decorator wrapping `inner`, using
    /// `log_store` as the sidecar log. Runs crash recovery before
    /// returning.
    pub fn new(
        inner: S,
        log_store: L,
        retention_policy: LogRetentionPolicy,
        commit_mode: CommitMode,
        trace: bool,
    ) -> Result<Self, LinkError> {
        let mut decorator = Self {
            inner,
            log_store,
            log: Vec::new(),
            committed: HashSet::new(),
            rolled_back: HashSet::new(),
            applied: HashSet::new(),
            current: None,
            sequence_counter: 0,
            applied_sequence: 0,
            retention_policy,
            commit_mode,
            replaying: false,
            trace,
        };
        decorator.recover()?;
        Ok(decorator)
    }

    /// Conventional sidecar filename for the transitions log.
    pub fn make_transitions_database_filename<P: AsRef<Path>>(database_filename: P) -> PathBuf {
        let path = database_filename.as_ref();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let name = format!("{stem}.transitions.links");
        match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
            _ => PathBuf::from(name),
        }
    }

    pub fn retention_policy(&self) -> &LogRetentionPolicy {
        &self.retention_policy
    }

    pub fn set_retention_policy(&mut self, policy: LogRetentionPolicy) {
        self.retention_policy = policy;
    }

    pub fn commit_mode(&self) -> CommitMode {
        self.commit_mode
    }

    pub fn set_commit_mode(&mut self, mode: CommitMode) {
        self.commit_mode = mode;
    }

    pub fn applied_sequence(&self) -> i64 {
        self.applied_sequence
    }

    pub fn last_logged_sequence(&self) -> i64 {
        self.sequence_counter
    }

    /// Returns a snapshot of the transitions log in sequence order.
    pub fn log(&self) -> Vec<GenericTransition<T>> {
        self.log.clone()
    }

    pub fn inner(&self) -> &S {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    pub fn log_store(&self) -> &L {
        &self.log_store
    }

    pub fn log_store_mut(&mut self) -> &mut L {
        &mut self.log_store
    }

    pub fn into_inner(self) -> (S, L) {
        (self.inner, self.log_store)
    }

    /// Makes both the data store and the transitions log durable.
    pub fn flush(&mut self) -> Result<(), LinkError> {
        self.inner.flush()?;
        self.log_store.flush_log()?;
        Ok(())
    }

    /// Alias of [`flush`](Self::flush), kept for parity with the other
    /// decorators and with the C# API.
    pub fn save(&mut self) -> Result<(), LinkError> {
        self.flush()
    }

    /// Cheap check for "has another process written to the wrapped
    /// store since we last read or wrote it?".
    pub fn has_external_changes(&self) -> Result<bool, LinkError> {
        self.inner.has_external_changes()
    }

    /// Re-reads the wrapped store and rebuilds the transactions state
    /// from the log — the recovery path after another process wrote.
    pub fn reload(&mut self) -> Result<(), LinkError> {
        if self.current.is_some() {
            return Err(LinkError::Transaction(
                "Cannot reload while a transaction is open.".to_string(),
            ));
        }
        self.inner.reload()?;
        self.recover()
    }

    // ----- Write API ------------------------------------------------------

    pub fn create(&mut self, source: T, target: T) -> Result<T, LinkError> {
        if self.replaying {
            return self.inner.create_link(source, target);
        }
        let owns = self.ensure_open_transaction();
        let id = self.inner.create_link(source, target)?;
        let after = self
            .inner
            .get_link(id)
            .map(|link| GenericDoubletLink::from_link(&link))
            .unwrap_or_else(|| GenericDoubletLink::new(id, source, target));
        self.record_transition(TransitionKind::Create, GenericDoubletLink::empty(), after)?;
        if owns {
            self.commit_current()?;
        }
        Ok(id)
    }

    pub fn update(&mut self, id: T, source: T, target: T) -> Result<GenericLink<T>, LinkError> {
        if self.replaying {
            return self.inner.update_link(id, source, target);
        }
        let before = self.snapshot(id);
        let owns = self.ensure_open_transaction();
        let mut observed: Vec<ObservedChange<T>> = Vec::new();
        let outcome = self
            .inner
            .update_link_observed(id, source, target, &mut |before, after| {
                record_observed(&mut observed, before, after)
            });
        let prev = match outcome {
            Ok(prev) => prev,
            Err(err) => {
                if owns {
                    self.current = None;
                }
                return Err(err);
            }
        };
        if observed.is_empty() {
            let after = self
                .inner
                .get_link(id)
                .map(|link| GenericDoubletLink::from_link(&link))
                .unwrap_or_else(|| GenericDoubletLink::new(id, source, target));
            self.record_transition(TransitionKind::Update, before, after)?;
        } else {
            self.record_observed_transitions(&observed)?;
        }
        if owns {
            self.commit_current()?;
        }
        Ok(prev)
    }

    pub fn delete(&mut self, id: T) -> Result<GenericLink<T>, LinkError> {
        self.delete_observed(id, &mut |_, _| {})
    }

    /// [`Self::delete`], reporting every change the underlying store made.
    ///
    /// Deleting a link cascades into every link that still used it, and those
    /// deletions are changes of their own: the C# CLI hands
    /// `AdvancedMixedQueryProcessor.RemoveLinks` a handler that `links.Delete`
    /// calls once per removed link, so `--changes` lists the usages too. The
    /// observer is the same seam, threaded through the decorator stack.
    pub fn delete_observed(
        &mut self,
        id: T,
        observer: &mut dyn FnMut(GenericLink<T>, GenericLink<T>),
    ) -> Result<GenericLink<T>, LinkError> {
        if self.replaying {
            let deleted = self.inner.delete_link(id)?;
            observer(deleted, GenericLink::null());
            return Ok(deleted);
        }
        let before = self.snapshot(id);
        let owns = self.ensure_open_transaction();
        let mut observed: Vec<ObservedChange<T>> = Vec::new();
        let outcome = self.inner.delete_link_observed(id, &mut |before, after| {
            observer(before, after);
            record_observed(&mut observed, before, after)
        });
        let deleted = match outcome {
            Ok(d) => d,
            Err(err) => {
                if owns {
                    self.current = None;
                }
                return Err(err);
            }
        };
        if observed.is_empty() {
            self.record_transition(TransitionKind::Delete, before, GenericDoubletLink::empty())?;
        } else {
            self.record_observed_transitions(&observed)?;
        }
        if owns {
            self.commit_current()?;
        }
        Ok(deleted)
    }

    /// Composite create-and-update used by callers that want a link
    /// initialised with source/target in a single pair of transitions
    /// (matches the C# `CreateAndUpdate` extension semantics, which
    /// always emits a Create followed by an Update transition).
    pub fn create_and_update(&mut self, source: T, target: T) -> Result<T, LinkError> {
        let owns = self.ensure_open_transaction();
        let zero = T::from_byte(0);
        let id = self.create(zero, zero)?;
        self.update(id, source, target)?;
        if owns {
            self.commit_current()?;
        }
        Ok(id)
    }

    pub fn exists(&self, id: T) -> bool {
        self.inner.link_exists(id)
    }

    pub fn search(&self, source: T, target: T) -> Option<T> {
        self.inner.search_link(source, target)
    }

    pub fn get_or_create(&mut self, source: T, target: T) -> Result<T, LinkError> {
        if let Some(existing) = self.inner.search_link(source, target) {
            return Ok(existing);
        }
        self.create(source, target)
    }

    pub fn ensure_created(&mut self, id: T) -> Result<T, LinkError> {
        // ensure_created is used by recovery/replay only and is not
        // itself a logical write; bypass transition recording.
        self.inner.ensure_link_created(id)
    }

    /// Current state of `id` as a doublet, or an empty one at `id`.
    fn snapshot(&self, id: T) -> GenericDoubletLink<T> {
        let zero = T::from_byte(0);
        self.inner
            .get_link(id)
            .map(|link| GenericDoubletLink::from_link(&link))
            .unwrap_or_else(|| GenericDoubletLink::new(id, zero, zero))
    }

    fn ensure_open_transaction(&mut self) -> bool {
        if self.current.is_none() {
            self.current = Some(PendingTransaction {
                id: new_transaction_id(),
                transitions: Vec::new(),
                auto_commit: true,
                started_ms: now_unix_ms(),
            });
            true
        } else {
            false
        }
    }

    /// Writes one transition per link a single logical write touched.
    ///
    /// A resolved write is not necessarily a single-link change: the
    /// upstream uniqueness and usages decorators merge duplicates and
    /// cascade through usages, so one `update`/`delete` call can rewrite
    /// or remove several links. Each of them needs its own transition,
    /// otherwise a rollback (or a version-control branch switch, which
    /// replays the same transitions) cannot restore the links the
    /// cascade touched.
    ///
    /// The kind is derived from the observed pair rather than taken from
    /// the outer operation, because a cascade can delete a link during an
    /// `update` — recording that as an `Update` would make the revert a
    /// no-op, since the link no longer exists to be updated back.
    fn record_observed_transitions(
        &mut self,
        observed: &[ObservedChange<T>],
    ) -> Result<(), LinkError> {
        let zero = T::from_byte(0);
        for (_, before, after) in observed {
            let kind = match (before.index != zero, after.index != zero) {
                (false, true) => TransitionKind::Create,
                (true, false) => TransitionKind::Delete,
                _ => TransitionKind::Update,
            };
            self.record_transition(kind, *before, *after)?;
        }
        Ok(())
    }

    fn record_transition(
        &mut self,
        kind: TransitionKind,
        before: GenericDoubletLink<T>,
        after: GenericDoubletLink<T>,
    ) -> Result<(), LinkError> {
        self.sequence_counter += 1;
        let sequence = self.sequence_counter;
        let timestamp_ms = now_unix_ms();
        let transaction_id = self.current.as_ref().map(|tx| tx.id).ok_or_else(|| {
            LinkError::Transaction(
                "internal: missing open transaction while recording transition".to_string(),
            )
        })?;
        let transition = GenericTransition {
            transaction_id,
            sequence,
            timestamp_ms,
            kind,
            before,
            after,
        };
        if let Some(current) = self.current.as_mut() {
            current.transitions.push(transition);
        }
        self.log.push(transition);
        self.write_transition_to_log(&transition)?;
        if self.trace {
            eprintln!(
                "[Transactions] Recorded {:?} seq={} tx={:032x}: ({},{},{}) -> ({},{},{}).",
                kind,
                sequence,
                transaction_id,
                before.index,
                before.source,
                before.target,
                after.index,
                after.source,
                after.target,
            );
        }
        Ok(())
    }

    fn write_transition_to_log(
        &mut self,
        transition: &GenericTransition<T>,
    ) -> Result<(), LinkError> {
        self.log_store.append_log_entry(&format!(
            "{TRANSITION_NAME_PREFIX}{}",
            transition.serialize()
        ))
    }

    fn write_marker(&mut self, name: &str) -> Result<(), LinkError> {
        self.log_store.append_log_entry(name)
    }

    // ----- Transaction handle --------------------------------------------

    pub fn begin_transaction(&mut self) -> Result<TransactionHandle, LinkError> {
        if self.current.is_some() {
            return Err(LinkError::Transaction(
                "Nested transactions are not supported.".to_string(),
            ));
        }
        let id = new_transaction_id();
        let started_ms = now_unix_ms();
        self.current = Some(PendingTransaction {
            id,
            transitions: Vec::new(),
            auto_commit: false,
            started_ms,
        });
        Ok(TransactionHandle { id, started_ms })
    }

    pub fn commit(&mut self) -> Result<(), LinkError> {
        if self.current.is_none() {
            return Ok(());
        }
        self.commit_current()
    }

    fn commit_current(&mut self) -> Result<(), LinkError> {
        let pending = match self.current.take() {
            Some(p) => p,
            None => return Ok(()),
        };
        self.committed.insert(pending.id);
        self.write_marker(&format!("{COMMIT_MARKER_PREFIX}{:032x}", pending.id))?;
        if self.trace {
            eprintln!(
                "[Transactions] Committed tx {:032x} (mode={:?}, transitions={}).",
                pending.id,
                self.commit_mode,
                pending.transitions.len()
            );
        }
        for transition in &pending.transitions {
            self.mark_applied(transition)?;
        }
        let _ = pending.auto_commit;
        let _ = pending.started_ms;
        self.enforce_retention()?;
        Ok(())
    }

    pub fn rollback(&mut self) -> Result<(), LinkError> {
        let pending = match self.current.take() {
            Some(p) => p,
            None => return Ok(()),
        };
        self.rolled_back.insert(pending.id);
        self.replaying = true;
        for transition in pending.transitions.iter().rev() {
            self.try_revert_transition(transition);
        }
        self.replaying = false;
        self.write_marker(&format!("{ROLLBACK_MARKER_PREFIX}{:032x}", pending.id))?;
        if self.trace {
            eprintln!(
                "[Transactions] Rolled back tx {:032x} ({} transitions).",
                pending.id,
                pending.transitions.len(),
            );
        }
        self.enforce_retention()?;
        Ok(())
    }

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

    fn try_apply_transition(&mut self, transition: &GenericTransition<T>, record_applied: bool) {
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

    fn try_revert_transition(&mut self, transition: &GenericTransition<T>) {
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

    fn mark_applied(&mut self, transition: &GenericTransition<T>) -> Result<(), LinkError> {
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

    fn enforce_retention(&mut self) -> Result<(), LinkError> {
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

/// Read paths that lend out references, available whenever the wrapped
/// store keeps its links resident in memory.
impl<T, S, L> GenericTransactionsDecorator<T, S, L>
where
    T: LinkReference,
    S: LinksStorageRef<T>,
    L: TransitionLogStore,
{
    pub fn get(&self, id: T) -> Option<&GenericLink<T>> {
        self.inner.get_link_ref(id)
    }

    pub fn all(&self) -> Vec<&GenericLink<T>> {
        self.inner.all_link_refs()
    }

    pub fn query(
        &self,
        index: Option<T>,
        source: Option<T>,
        target: Option<T>,
    ) -> Vec<&GenericLink<T>> {
        self.inner.query_link_refs(index, source, target)
    }
}

// ----- Helpers ----------------------------------------------------------

fn insert_ordered<T: LinkReference>(
    list: &mut Vec<GenericTransition<T>>,
    transition: GenericTransition<T>,
) {
    let mut lo = 0usize;
    let mut hi = list.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if list[mid].sequence < transition.sequence {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    list.insert(lo, transition);
}

static TX_COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_transaction_id() -> u128 {
    // Combine a per-process counter with the current timestamp to
    // approximate a Guid without pulling in the `uuid` crate.
    let count = TX_COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
    let now = now_unix_ms() as u128;
    (now << 64) | count
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_policy_parses_specs() {
        assert!(matches!(
            LogRetentionPolicy::parse("infinite").unwrap(),
            LogRetentionPolicy::Infinite
        ));
        assert!(matches!(
            LogRetentionPolicy::parse("sized:1000").unwrap(),
            LogRetentionPolicy::Sized {
                max_transitions: 1000
            }
        ));
        match LogRetentionPolicy::parse("chunked:500:/tmp/x").unwrap() {
            LogRetentionPolicy::Chunked {
                chunk_size,
                archive_directory,
            } => {
                assert_eq!(chunk_size, 500);
                assert_eq!(archive_directory, PathBuf::from("/tmp/x"));
            }
            _ => panic!("expected Chunked"),
        }
        assert!(LogRetentionPolicy::parse("garbage").is_err());
    }

    #[test]
    fn transition_round_trips_through_serialize() {
        let t = Transition {
            transaction_id: 0xabcdef1234567890u128,
            sequence: 42,
            timestamp_ms: 1234567890,
            kind: TransitionKind::Update,
            before: DoubletLink::new(1, 2, 3),
            after: DoubletLink::new(1, 4, 5),
        };
        let parsed = Transition::try_parse(&t.serialize()).unwrap();
        assert_eq!(t, parsed);
    }

    #[test]
    fn wide_transition_is_rejected_by_a_narrow_address_type() {
        let wide = GenericTransition::<u64> {
            transaction_id: 7,
            sequence: 1,
            timestamp_ms: 0,
            kind: TransitionKind::Create,
            before: GenericDoubletLink::empty(),
            after: GenericDoubletLink::new(u32::MAX as u64 + 1, 0, 0),
        };
        assert!(matches!(
            GenericTransition::<u32>::parse(&wide.serialize()),
            Err(LinkError::AddressOutOfRange(_))
        ));
        assert!(GenericTransition::<u64>::parse(&wide.serialize()).is_ok());
    }
}
