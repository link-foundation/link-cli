//! Optional transactions layer for the Rust link-cli.
//!
//! Mirrors the C# `TransactionsDecorator` in
//! `csharp/Foundation.Data.Doublets.Cli.Library/TransactionsDecorator.cs`.
//!
//! The decorator wraps a [`NamedTypesDecorator`] and records every
//! `create` / `update` / `delete` as a reversible [`Transition`] in a
//! sidecar doublets log store. Supports explicit transactions, sync
//! commits, three retention policies, and crash recovery (R1-R7, R10).
//!
//! Optional — when not opted in, the bare [`NamedTypesDecorator`]
//! behaves identically (R8, R9, R17).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};

use crate::link::Link;
use crate::named_types::{NamedTypes, NamedTypesDecorator};

/// The kind of write operation recorded by a [`Transition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransitionKind {
    Create,
    Update,
    Delete,
}

impl TransitionKind {
    fn as_u8(self) -> u8 {
        match self {
            TransitionKind::Create => 0,
            TransitionKind::Update => 1,
            TransitionKind::Delete => 2,
        }
    }

    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(TransitionKind::Create),
            1 => Some(TransitionKind::Update),
            2 => Some(TransitionKind::Delete),
            _ => None,
        }
    }
}

/// Sync flushes data-store side-effects before `commit` returns.
///
/// Async durably persists the transitions then applies the data-store
/// side-effects on a background-friendly path (already-applied
/// side-effects are the common case for in-process inner stores).
///
/// The Rust port runs both modes synchronously on the calling thread
/// for predictability; the distinction is preserved for parity with C#
/// and for future expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommitMode {
    #[default]
    Sync,
    Async,
}

/// Retention policy for the transitions log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogRetentionPolicy {
    /// Keep every transition forever (default).
    Infinite,
    /// Drop the oldest applied transitions once the live log exceeds
    /// `max_transitions`. Never drops un-applied transitions (R7).
    Sized { max_transitions: u64 },
    /// Archive the oldest `chunk_size` applied transitions to a
    /// rolling file in `archive_directory` once the live log reaches
    /// `chunk_size`.
    Chunked {
        chunk_size: u64,
        archive_directory: PathBuf,
    },
}

impl Default for LogRetentionPolicy {
    fn default() -> Self {
        Self::Infinite
    }
}

impl LogRetentionPolicy {
    /// Parses a CLI spec: `infinite`, `sized:<n>`, `chunked:<n>:<dir>`.
    pub fn parse(spec: &str) -> Result<Self> {
        let trimmed = spec.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("infinite") {
            return Ok(Self::Infinite);
        }

        let lowered = trimmed.to_ascii_lowercase();
        if lowered.starts_with("sized:") {
            let rest = &trimmed["sized:".len()..];
            let max: u64 = rest
                .parse()
                .map_err(|_| anyhow!("invalid sized retention spec '{spec}'"))?;
            return Ok(Self::Sized {
                max_transitions: max,
            });
        }
        if lowered.starts_with("chunked:") {
            let rest = &trimmed["chunked:".len()..];
            let (size_text, dir) = rest
                .split_once(':')
                .ok_or_else(|| anyhow!("invalid chunked retention spec '{spec}'"))?;
            let chunk_size: u64 = size_text
                .parse()
                .map_err(|_| anyhow!("invalid chunked size in '{spec}'"))?;
            if chunk_size == 0 {
                bail!("invalid chunked size in '{spec}'");
            }
            if dir.is_empty() {
                bail!("invalid chunked retention spec '{spec}'");
            }
            return Ok(Self::Chunked {
                chunk_size,
                archive_directory: PathBuf::from(dir),
            });
        }
        bail!("unknown retention spec '{spec}'");
    }
}

/// A single doublet link state captured by a transition (mirror of the
/// C# `Platform.Data.Doublets.Link<uint>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct DoubletLink {
    pub index: u32,
    pub source: u32,
    pub target: u32,
}

impl DoubletLink {
    pub const fn new(index: u32, source: u32, target: u32) -> Self {
        Self {
            index,
            source,
            target,
        }
    }

    pub const fn empty() -> Self {
        Self::new(0, 0, 0)
    }

    pub fn from_link(link: &Link) -> Self {
        Self::new(link.index, link.source, link.target)
    }
}

/// Reversible write captured by the transactions layer. Holds both
/// `before` and `after` link states so the operation can be undone or
/// replayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Transition {
    pub transaction_id: u128,
    pub sequence: i64,
    pub timestamp_ms: i64,
    pub kind: TransitionKind,
    pub before: DoubletLink,
    pub after: DoubletLink,
}

impl Transition {
    pub(crate) const SCHEMA_VERSION: &'static str = "v1";

    /// Encodes the transition as a single line stored as the *name*
    /// of one link in the log doublets store.
    pub fn serialize(&self) -> String {
        format!(
            "{schema}|{tx:032x}|{seq}|{ms}|{kind}|{bi},{bs},{bt}|{ai},{as_},{at}",
            schema = Self::SCHEMA_VERSION,
            tx = self.transaction_id,
            seq = self.sequence,
            ms = self.timestamp_ms,
            kind = self.kind.as_u8(),
            bi = self.before.index,
            bs = self.before.source,
            bt = self.before.target,
            ai = self.after.index,
            as_ = self.after.source,
            at = self.after.target,
        )
    }

    /// Parses a serialized transition.
    pub fn try_parse(text: &str) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        let parts: Vec<&str> = text.split('|').collect();
        if parts.len() < 7 {
            return None;
        }
        if parts[0] != Self::SCHEMA_VERSION {
            return None;
        }
        let tx = u128::from_str_radix(parts[1], 16).ok()?;
        let seq: i64 = parts[2].parse().ok()?;
        let ms: i64 = parts[3].parse().ok()?;
        let kind_value: u8 = parts[4].parse().ok()?;
        let kind = TransitionKind::from_u8(kind_value)?;
        let before = parse_doublet(parts[5])?;
        let after = parse_doublet(parts[6])?;
        Some(Self {
            transaction_id: tx,
            sequence: seq,
            timestamp_ms: ms,
            kind,
            before,
            after,
        })
    }
}

fn parse_doublet(text: &str) -> Option<DoubletLink> {
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    Some(DoubletLink {
        index: parts[0].parse().ok()?,
        source: parts[1].parse().ok()?,
        target: parts[2].parse().ok()?,
    })
}

/// Sidecar-store name prefixes used by the recovery protocol.
pub(crate) const COMMIT_MARKER_PREFIX: &str = "__transactions:commit:";
pub(crate) const ROLLBACK_MARKER_PREFIX: &str = "__transactions:rollback:";
pub(crate) const APPLIED_MARKER_PREFIX: &str = "__transactions:applied:";
pub(crate) const TRANSITION_NAME_PREFIX: &str = "__transactions:transition:";

/// Pending state of a transaction (used by the explicit transaction
/// handle and by per-write auto-transactions).
struct PendingTransaction {
    id: u128,
    transitions: Vec<Transition>,
    auto_commit: bool,
    started_ms: i64,
}

/// Snapshot of an open transaction (returned by [`TransactionsDecorator::begin_transaction`]).
#[derive(Debug, Clone)]
pub struct TransactionHandle {
    pub id: u128,
    pub started_ms: i64,
}

/// The transactions decorator wraps a [`NamedTypesDecorator`] and
/// records every write as a reversible [`Transition`] in `log_store`.
pub struct TransactionsDecorator {
    inner: NamedTypesDecorator,
    log_store: NamedTypesDecorator,
    log: Vec<Transition>,
    committed: HashSet<u128>,
    rolled_back: HashSet<u128>,
    applied: HashSet<i64>,
    current: Option<PendingTransaction>,
    sequence_counter: i64,
    applied_sequence: i64,
    retention_policy: LogRetentionPolicy,
    commit_mode: CommitMode,
    replaying: bool,
    trace: bool,
}

impl TransactionsDecorator {
    /// Creates a new transactions decorator wrapping `inner`, using
    /// `log_store` as the sidecar log store.
    pub fn new(
        inner: NamedTypesDecorator,
        log_store: NamedTypesDecorator,
        retention_policy: LogRetentionPolicy,
        commit_mode: CommitMode,
        trace: bool,
    ) -> Result<Self> {
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
    pub fn log(&self) -> Vec<Transition> {
        self.log.clone()
    }

    pub fn inner(&self) -> &NamedTypesDecorator {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut NamedTypesDecorator {
        &mut self.inner
    }

    pub fn log_store(&self) -> &NamedTypesDecorator {
        &self.log_store
    }

    pub fn log_store_mut(&mut self) -> &mut NamedTypesDecorator {
        &mut self.log_store
    }

    pub fn into_inner(self) -> (NamedTypesDecorator, NamedTypesDecorator) {
        (self.inner, self.log_store)
    }

    pub fn save(&self) -> Result<()> {
        self.inner.save()?;
        self.log_store.save()?;
        Ok(())
    }

    // ----- Write API ------------------------------------------------------

    pub fn create(&mut self, source: u32, target: u32) -> Result<u32> {
        if self.replaying {
            return Ok(self.inner.create(source, target));
        }
        let owns = self.ensure_open_transaction();
        let id = self.inner.create(source, target);
        let after = self
            .inner
            .get(id)
            .map(DoubletLink::from_link)
            .unwrap_or_else(|| DoubletLink::new(id, source, target));
        self.record_transition(TransitionKind::Create, DoubletLink::empty(), after)?;
        if owns {
            self.commit_current()?;
        }
        Ok(id)
    }

    pub fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link> {
        if self.replaying {
            return self.inner.update(id, source, target);
        }
        let before = self
            .inner
            .get(id)
            .map(DoubletLink::from_link)
            .unwrap_or_else(|| DoubletLink::new(id, 0, 0));
        let owns = self.ensure_open_transaction();
        let prev = match self.inner.update(id, source, target) {
            Ok(prev) => prev,
            Err(err) => {
                if owns {
                    self.current = None;
                }
                return Err(err);
            }
        };
        let after = self
            .inner
            .get(id)
            .map(DoubletLink::from_link)
            .unwrap_or_else(|| DoubletLink::new(id, source, target));
        self.record_transition(TransitionKind::Update, before, after)?;
        if owns {
            self.commit_current()?;
        }
        Ok(prev)
    }

    pub fn delete(&mut self, id: u32) -> Result<Link> {
        if self.replaying {
            return self.inner.delete(id);
        }
        let before = self
            .inner
            .get(id)
            .map(DoubletLink::from_link)
            .unwrap_or_else(|| DoubletLink::new(id, 0, 0));
        let owns = self.ensure_open_transaction();
        let deleted = match self.inner.delete(id) {
            Ok(d) => d,
            Err(err) => {
                if owns {
                    self.current = None;
                }
                return Err(err);
            }
        };
        self.record_transition(TransitionKind::Delete, before, DoubletLink::empty())?;
        if owns {
            self.commit_current()?;
        }
        Ok(deleted)
    }

    /// Composite create-and-update used by callers that want a link
    /// initialised with source/target in a single pair of transitions
    /// (matches the C# `CreateAndUpdate` extension semantics, which
    /// always emits a Create followed by an Update transition).
    pub fn create_and_update(&mut self, source: u32, target: u32) -> Result<u32> {
        let owns = self.ensure_open_transaction();
        let id = self.create(0, 0)?;
        self.update(id, source, target)?;
        if owns {
            self.commit_current()?;
        }
        Ok(id)
    }

    pub fn exists(&self, id: u32) -> bool {
        self.inner.exists(id)
    }

    pub fn get(&self, id: u32) -> Option<&Link> {
        self.inner.get(id)
    }

    pub fn all(&self) -> Vec<&Link> {
        self.inner.all()
    }

    pub fn query(
        &self,
        index: Option<u32>,
        source: Option<u32>,
        target: Option<u32>,
    ) -> Vec<&Link> {
        self.inner.query(index, source, target)
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

    fn record_transition(
        &mut self,
        kind: TransitionKind,
        before: DoubletLink,
        after: DoubletLink,
    ) -> Result<()> {
        self.sequence_counter += 1;
        let sequence = self.sequence_counter;
        let timestamp_ms = now_unix_ms();
        let transaction_id = self
            .current
            .as_ref()
            .map(|tx| tx.id)
            .ok_or_else(|| anyhow!("internal: missing open transaction while recording transition"))?;
        let transition = Transition {
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

    fn write_transition_to_log(&mut self, transition: &Transition) -> Result<()> {
        // Always allocate a fresh link so each transition has its own
        // log entry (mirrors C# `CreateAndUpdate(Null, Null)`).
        let link = self.log_store.create(0, 0);
        let name = format!("{TRANSITION_NAME_PREFIX}{}", transition.serialize());
        self.log_store.set_name(link, &name)?;
        Ok(())
    }

    fn write_marker(&mut self, name: &str) -> Result<()> {
        // Always allocate a fresh link so markers do not overwrite one
        // another (mirrors C# `CreateAndUpdate(Null, Null)`).
        let link = self.log_store.create(0, 0);
        self.log_store.set_name(link, name)?;
        Ok(())
    }

    // ----- Transaction handle --------------------------------------------

    pub fn begin_transaction(&mut self) -> Result<TransactionHandle> {
        if self.current.is_some() {
            bail!("Nested transactions are not supported.");
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

    pub fn commit(&mut self) -> Result<()> {
        if self.current.is_none() {
            return Ok(());
        }
        self.commit_current()
    }

    fn commit_current(&mut self) -> Result<()> {
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

    pub fn rollback(&mut self) -> Result<()> {
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
    pub fn apply_transition(&mut self, transition: &Transition) {
        self.replaying = true;
        self.try_apply_transition(transition, false);
        self.replaying = false;
    }

    /// Public helper for higher-level decorators (e.g. version control)
    /// — reverts a single transition without writing a new log entry.
    pub fn revert_transition(&mut self, transition: &Transition) {
        self.replaying = true;
        self.try_revert_transition(transition);
        self.replaying = false;
    }

    fn try_apply_transition(&mut self, transition: &Transition, record_applied: bool) {
        let result: Result<()> = match transition.kind {
            TransitionKind::Create => {
                if transition.after.index != 0 && !self.inner.exists(transition.after.index) {
                    self.inner.ensure_created(transition.after.index);
                    self.inner
                        .update(
                            transition.after.index,
                            transition.after.source,
                            transition.after.target,
                        )
                        .map(|_| ())
                } else {
                    Ok(())
                }
            }
            TransitionKind::Update => {
                if transition.after.index != 0 && self.inner.exists(transition.after.index) {
                    self.inner
                        .update(
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
                if transition.before.index != 0 && self.inner.exists(transition.before.index) {
                    self.inner.delete(transition.before.index).map(|_| ())
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

    fn try_revert_transition(&mut self, transition: &Transition) {
        let result = match transition.kind {
            TransitionKind::Create => {
                if transition.after.index != 0 && self.inner.exists(transition.after.index) {
                    self.inner.delete(transition.after.index).map(|_| ())
                } else {
                    Ok(())
                }
            }
            TransitionKind::Update => {
                if transition.before.index != 0 && self.inner.exists(transition.before.index) {
                    self.inner
                        .update(
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
                if transition.before.index != 0 && !self.inner.exists(transition.before.index) {
                    self.inner.ensure_created(transition.before.index);
                    self.inner
                        .update(
                            transition.before.index,
                            transition.before.source,
                            transition.before.target,
                        )
                        .map(|_| ())
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

    fn mark_applied(&mut self, transition: &Transition) -> Result<()> {
        if self.applied.insert(transition.sequence) {
            self.write_marker(&format!(
                "{APPLIED_MARKER_PREFIX}{}",
                transition.sequence
            ))?;
            if transition.sequence > self.applied_sequence {
                self.applied_sequence = transition.sequence;
            }
        }
        Ok(())
    }

    // ----- Recovery -------------------------------------------------------

    /// Rebuilds the in-memory log and marker tables from the sidecar
    /// log store and re-applies committed-but-unapplied side-effects.
    pub fn recover(&mut self) -> Result<()> {
        self.log.clear();
        self.committed.clear();
        self.rolled_back.clear();
        self.applied.clear();
        self.sequence_counter = 0;
        self.applied_sequence = 0;

        // Read every named link from the log store.
        let all_links: Vec<Link> = self
            .log_store
            .all()
            .into_iter()
            .copied()
            .collect();
        for link in &all_links {
            let name = match self.log_store.get_name(link.index)? {
                Some(value) => value,
                None => continue,
            };
            if let Some(payload) = name.strip_prefix(TRANSITION_NAME_PREFIX) {
                if let Some(transition) = Transition::try_parse(payload) {
                    insert_ordered(&mut self.log, transition);
                    if transition.sequence > self.sequence_counter {
                        self.sequence_counter = transition.sequence;
                    }
                }
            } else if let Some(rest) = name.strip_prefix(COMMIT_MARKER_PREFIX) {
                if let Ok(tx_id) = u128::from_str_radix(rest, 16) {
                    self.committed.insert(tx_id);
                }
            } else if let Some(rest) = name.strip_prefix(ROLLBACK_MARKER_PREFIX) {
                if let Ok(tx_id) = u128::from_str_radix(rest, 16) {
                    self.rolled_back.insert(tx_id);
                }
            } else if let Some(rest) = name.strip_prefix(APPLIED_MARKER_PREFIX) {
                if let Ok(seq) = rest.parse::<i64>() {
                    self.applied.insert(seq);
                    if seq > self.applied_sequence {
                        self.applied_sequence = seq;
                    }
                }
            }
        }

        // Re-apply committed-but-not-applied transitions (crash mid-async).
        let log_snapshot: Vec<Transition> = self.log.clone();
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

    fn enforce_retention(&mut self) -> Result<()> {
        match self.retention_policy.clone() {
            LogRetentionPolicy::Infinite => Ok(()),
            LogRetentionPolicy::Sized { max_transitions } => self.enforce_sized(max_transitions),
            LogRetentionPolicy::Chunked {
                chunk_size,
                archive_directory,
            } => self.enforce_chunked(chunk_size, &archive_directory),
        }
    }

    fn enforce_sized(&mut self, max_transitions: u64) -> Result<()> {
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

    fn enforce_chunked(&mut self, chunk_size: u64, archive_directory: &Path) -> Result<()> {
        if chunk_size == 0 {
            return Ok(());
        }
        if (self.log.len() as u64) < chunk_size {
            return Ok(());
        }
        let chunk: Vec<Transition> = self.log.iter().take(chunk_size as usize).copied().collect();
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
        std::fs::create_dir_all(archive_directory)
            .with_context(|| format!("failed to create archive dir {}", archive_directory.display()))?;
        let timestamp = now_unix_ms();
        let file_name =
            format!("transitions-chunk-{timestamp}-{:032x}.log", new_transaction_id());
        let path = archive_directory.join(file_name);
        use std::io::Write;
        let mut file = std::fs::File::create(&path)
            .with_context(|| format!("failed to create archive file {}", path.display()))?;
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

// ----- Helpers ----------------------------------------------------------

fn insert_ordered(list: &mut Vec<Transition>, transition: Transition) {
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
            LogRetentionPolicy::Sized { max_transitions: 1000 }
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
}
