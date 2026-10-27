//! Optional staged links engine: concurrency, immutable history, and a
//! replaceable commit log. See `docs/ENGINE-STRATEGIES.md` for costs and limits.
//!
//! Unlike the legacy undo decorator, no underlying mapped data is modified
//! while a transaction is open. A checksummed logical revision is flushed
//! before it becomes visible. The log is the authoritative storage.
#[cfg(test)]
mod boundary_tests;
mod branch;
#[cfg(feature = "causal")]
pub mod causal;
#[cfg(feature = "causal-batches")]
pub mod causal_batch;
#[cfg(feature = "causal-merkle")]
pub mod causal_merkle;
#[cfg(feature = "causal-projection")]
pub mod causal_projection;
#[cfg(feature = "causal")]
pub mod causal_replica;
mod changes;
mod checkpoint;
pub mod cluster;
mod commit;
#[cfg(test)]
mod constraint_cost_tests;
mod constraint_index;
#[cfg(test)]
mod cost_tests;
mod delta;
#[cfg(feature = "etcd")]
pub mod etcd;
mod group;
mod journal;
#[cfg(feature = "physical-lsm")]
pub mod lsm;
pub mod maintenance;
mod named;
mod object_cache;
pub mod paged;
pub mod partitioned;
mod pessimistic;
pub mod reclaiming;
pub mod replication;
mod rows;
#[cfg(test)]
mod shared_cost_tests;
mod sorted_runs;
#[cfg(all(feature = "sqlite", not(target_arch = "wasm32")))]
pub mod sqlite;
mod transaction;
mod validation;

use crate::{GenericLink, LinkError, TransitionLogStore};
use doublets::data::LinkReference;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

pub use branch::Branch;
pub use delta::DeltaLog;
pub use group::GroupCommitOutcome;
pub use journal::MemoryLog;
use rows::Rows;
pub use transaction::EngineTransaction;
pub use validation::SerializableValidation;

/// In-memory revision representation, independent of durability and isolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StateStructure {
    /// Clone complete sorted rows for each changed revision (the existing path).
    #[default]
    Flat,
    /// Share immutable entries and path-copy an ordered tree for changed rows.
    Shared,
    /// Share immutable sorted runs; merge occupied event-count tiers on writes.
    SortedRuns,
}

/// Logical journal payload, independent of the physical log's encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RevisionEncoding {
    /// Existing complete engine1 revisions.
    #[default]
    Snapshot,
    /// Canonical engine2 changed-row records bound to the preceding revision.
    Changes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConcurrencyControl {
    #[default]
    Optimistic,
    /// A second transaction fails immediately rather than blocking indefinitely.
    Exclusive,
    /// Reserve written addresses until session end; competing writes fail fast.
    Pessimistic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IsolationLevel {
    ReadCommitted,
    Snapshot,
    /// Serializable commits; whole-revision validation is the default strategy.
    #[default]
    Serializable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryPolicy {
    #[default]
    None,
    /// Includes the current revision; active transactions pin their own snapshot.
    Last(usize),
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    /// Every nonzero source/target must reference a row in the committed state.
    References,
    /// Reject duplicate source/target pairs instead of silently merging them.
    UniquePairs,
    UniqueNames,
}

/// Constraint maintenance, independent of concurrency and serializable reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConstraintValidation {
    /// Scan the complete candidate; no retained constraint indexes.
    #[default]
    Scan,
    /// Retain immutable indexes only for selected constraints; update changed rows.
    Indexed,
}

#[derive(Debug, Clone, Default)]
pub struct EngineOptions {
    pub state_structure: StateStructure,
    pub revision_encoding: RevisionEncoding,
    pub concurrency: ConcurrencyControl,
    pub isolation: IsolationLevel,
    pub serializable_validation: SerializableValidation,
    pub history: HistoryPolicy,
    pub constraints: Vec<Constraint>,
    pub constraint_validation: ConstraintValidation,
    pub trace: bool,
    /// Reject volatile or unknown log adapters, rather than claiming durability.
    pub require_durable_flush: bool,
}

impl EngineOptions {
    fn tracks_generations(&self) -> bool {
        self.concurrency == ConcurrencyControl::Pessimistic
            || (self.concurrency == ConcurrencyControl::Optimistic
                && (self.isolation != IsolationLevel::Serializable
                    || self.serializable_validation == SerializableValidation::ReadSet))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row<T> {
    pub link: GenericLink<T>,
    pub name: Option<String>,
}

/// Immutable revision, safe to keep after history retention or transaction end.
#[derive(Debug, Clone)]
pub struct Snapshot<T> {
    pub revision: u64,
    rows: Arc<Rows<T>>,
}
impl<T: LinkReference> Snapshot<T> {
    pub fn links(&self) -> Vec<GenericLink<T>> {
        self.rows.values().map(|row| row.link).collect()
    }
    pub fn get(&self, id: T) -> Option<&Row<T>> {
        self.rows.get(&id)
    }
    /// Legacy sorted-map view. Shared snapshots lazily copy O(N) rows/names on
    /// the first call; snapshot clones share that cache. Prefer `iter` or `get`.
    pub fn rows(&self) -> &BTreeMap<T, Row<T>> {
        self.rows.materialize()
    }
    /// Sorted borrowed rows without materializing the legacy BTreeMap view.
    pub fn iter(&self) -> impl Iterator<Item = (&T, &Row<T>)> {
        self.rows.iter()
    }
}

struct State<T, L> {
    log: L,
    current: Snapshot<T>,
    history: BTreeMap<u64, (Snapshot<T>, u128)>,
    changed: BTreeMap<T, u64>,
    write_owners: BTreeMap<T, Arc<()>>,
    next: u128,
    committed_next: u128,
    active: usize,
    poisoned: bool,
    options: EngineOptions,
    constraint_index: Option<constraint_index::ConstraintIndex<T>>,
}

/// Share clones of this handle across threads. All writes must use its sessions;
/// opening multiple independent writers on the same log is not supported.
pub struct Engine<T, L> {
    state: Arc<Mutex<State<T, L>>>,
}
impl<T, L> Clone for Engine<T, L> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

fn error(message: &str) -> LinkError {
    LinkError::Transaction(message.into())
}

impl<T: LinkReference, L: TransitionLogStore> Engine<T, L> {
    pub fn open(mut log: L, options: EngineOptions) -> Result<Self, LinkError> {
        if options.serializable_validation == SerializableValidation::ReadSet
            && options.isolation != IsolationLevel::Serializable
        {
            return Err(error("read-set validation requires serializable isolation"));
        }
        if options.require_durable_flush && !log.supports_durable_flush() {
            return Err(error("selected log does not support durable flush"));
        }
        if options.history == HistoryPolicy::Last(0) {
            return Err(error("history limit must be positive"));
        }
        let entries = log.read_log_entries()?;
        let mut state = State {
            current: Snapshot {
                revision: 0,
                rows: Arc::new(Rows::new(options.state_structure)),
            },
            history: BTreeMap::new(),
            changed: BTreeMap::new(),
            write_owners: BTreeMap::new(),
            next: 1,
            committed_next: 1,
            active: 0,
            poisoned: false,
            options,
            constraint_index: None,
            log,
        };
        state.retain();
        for (index, entry) in entries.into_iter().enumerate() {
            let (snapshot, next) = if index == 0 && entry.starts_with("engine3|") {
                let decoded = journal::decode_checkpoint(&entry, state.options.state_structure)?;
                state.history.clear();
                decoded
            } else {
                journal::decode_after(
                    &entry,
                    &state.current,
                    state.next,
                    state.options.state_structure,
                )?
            };
            state.validate(&snapshot.rows)?;
            state.next = next;
            state.committed_next = next;
            state.current = snapshot;
            state.retain();
        }
        // No transaction predates open, so recovered rows need no conflict
        // generations. Track only changes made while a session can be stale.
        if state.options.constraint_validation == ConstraintValidation::Indexed {
            state.constraint_index = constraint_index::ConstraintIndex::build(
                &state.options.constraints,
                &state.current.rows,
            )?;
        }
        Ok(Self {
            state: Arc::new(Mutex::new(state)),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, State<T, L>>, LinkError> {
        let state = self
            .state
            .lock()
            .map_err(|_| error("engine mutex poisoned"))?;
        if state.poisoned {
            return Err(error(
                "indeterminate journal outcome; close and reopen before continuing",
            ));
        }
        Ok(state)
    }
    pub fn snapshot(&self) -> Result<Snapshot<T>, LinkError> {
        Ok(self.lock()?.current.clone())
    }
    pub fn snapshot_at(&self, revision: u64) -> Result<Snapshot<T>, LinkError> {
        let state = self.lock()?;
        if revision == state.current.revision {
            return Ok(state.current.clone());
        }
        state
            .history
            .get(&revision)
            .map(|(snapshot, _)| snapshot.clone())
            .ok_or_else(|| error("revision is not retained"))
    }
    pub fn begin(&self) -> Result<EngineTransaction<T, L>, LinkError> {
        let mut state = self.lock()?;
        if state.options.concurrency == ConcurrencyControl::Exclusive && state.active > 0 {
            return Err(error("busy: an exclusive transaction is active"));
        }
        state.active += 1;
        Ok(EngineTransaction {
            engine: self.clone(),
            base: state.current.clone(),
            writes: BTreeMap::new(),
            closed: false,
            isolation: state.options.isolation,
            reads: (state.options.serializable_validation == SerializableValidation::ReadSet
                && state.options.concurrency != ConcurrencyControl::Exclusive)
                .then(|| Mutex::new(validation::ReadSet::default())),
            locks: (state.options.concurrency == ConcurrencyControl::Pessimistic)
                .then(pessimistic::WriteLocks::default),
        })
    }
    /// Run a caller's replication/catch-up/diagnostics operation under the same
    /// mutex as commits. An error poisons the engine because its outcome may be unknown.
    pub fn with_log<R>(
        &self,
        operation: impl FnOnce(&mut L) -> Result<R, LinkError>,
    ) -> Result<R, LinkError> {
        let mut state = self.lock()?;
        match operation(&mut state.log) {
            Ok(result) => Ok(result),
            Err(error) => {
                state.poisoned = true;
                Err(error)
            }
        }
    }
    /// Extract the authoritative log once every other handle/session is dropped.
    pub fn into_log(self) -> Result<L, Self> {
        match Arc::try_unwrap(self.state) {
            Ok(mutex) => Ok(mutex
                .into_inner()
                .unwrap_or_else(|poison| poison.into_inner())
                .log),
            Err(state) => Err(Self { state }),
        }
    }
}

impl<T: LinkReference, L> State<T, L> {
    fn retain(&mut self) {
        match self.options.history {
            HistoryPolicy::None => {}
            HistoryPolicy::All => {
                self.history.insert(
                    self.current.revision,
                    (self.current.clone(), self.committed_next),
                );
            }
            HistoryPolicy::Last(limit) => {
                self.history.insert(
                    self.current.revision,
                    (self.current.clone(), self.committed_next),
                );
                while self.history.len() > limit {
                    self.history.pop_first();
                }
            }
        }
    }
    fn validate(&self, rows: &Rows<T>) -> Result<(), LinkError> {
        let zero = T::from_byte(0);
        for constraint in &self.options.constraints {
            match constraint {
                Constraint::References => {
                    for row in rows.values() {
                        if [row.link.source, row.link.target]
                            .iter()
                            .any(|id| *id != zero && !rows.contains_key(id))
                        {
                            return Err(error("constraint: dangling link reference"));
                        }
                    }
                }
                Constraint::UniquePairs => {
                    let mut pairs = std::collections::BTreeSet::new();
                    for row in rows.values() {
                        if !pairs.insert((row.link.source, row.link.target)) {
                            return Err(error("constraint: duplicate pair"));
                        }
                    }
                }
                Constraint::UniqueNames => {
                    let mut names = std::collections::BTreeSet::new();
                    for name in rows.values().filter_map(|row| row.name.as_ref()) {
                        if !names.insert(name) {
                            return Err(error("constraint: duplicate name"));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
