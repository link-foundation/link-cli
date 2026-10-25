//! Optional staged links engine: concurrency, immutable history, and a
//! replaceable commit log. See `docs/ENGINE-STRATEGIES.md` for costs and limits.
//!
//! Unlike the legacy undo decorator, no underlying mapped data is modified
//! while a transaction is open. A complete checksummed revision is flushed
//! before it becomes visible. The log is the authoritative storage.
#[cfg(test)]
mod boundary_tests;
mod branch;
pub mod cluster;
mod commit;
#[cfg(test)]
mod cost_tests;
mod delta;
#[cfg(feature = "etcd")]
pub mod etcd;
mod group;
mod journal;
mod named;
pub mod partitioned;
mod pessimistic;
pub mod replication;
mod rows;
#[cfg(test)]
mod shared_cost_tests;
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

#[derive(Debug, Clone, Default)]
pub struct EngineOptions {
    pub state_structure: StateStructure,
    pub concurrency: ConcurrencyControl,
    pub isolation: IsolationLevel,
    pub serializable_validation: SerializableValidation,
    pub history: HistoryPolicy,
    pub constraints: Vec<Constraint>,
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
    history: BTreeMap<u64, Snapshot<T>>,
    changed: BTreeMap<T, u64>,
    write_owners: BTreeMap<T, Arc<()>>,
    next: u128,
    committed_next: u128,
    active: usize,
    poisoned: bool,
    options: EngineOptions,
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
            log,
        };
        state.retain();
        for entry in entries {
            let (mut snapshot, next) = journal::decode::<T>(&entry)?;
            if snapshot.revision
                != state
                    .current
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| error("revision overflow"))?
            {
                return Err(error("journal revisions are not contiguous"));
            }
            state.validate(&snapshot.rows)?;
            if next < state.next {
                return Err(error("journal allocator moved backwards"));
            }
            state.next = next;
            state.committed_next = next;
            snapshot.rows = Arc::new(
                Arc::unwrap_or_clone(snapshot.rows)
                    .adapt(state.options.state_structure, Some(&state.current.rows)),
            );
            state.current = snapshot;
            state.retain();
        }
        // No transaction predates open, so recovered rows need no conflict
        // generations. Track only changes made while a session can be stale.
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
            .cloned()
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
                self.history
                    .insert(self.current.revision, self.current.clone());
            }
            HistoryPolicy::Last(limit) => {
                self.history
                    .insert(self.current.revision, self.current.clone());
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
