//! Optional optimistic coordination across namespaced shards. A coordinator
//! must provide atomic snapshots/CAS; local workspaces are never shared state.
use super::{
    error, ConcurrencyControl, Engine, EngineOptions, EngineTransaction, IsolationLevel, MemoryLog,
};
use crate::{GenericLink, LinkError, LinksStorage};
use doublets::data::LinkReference;
use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JournalVersion {
    /// A non-reusable modification generation, zero only for an absent key.
    pub generation: u64,
    pub entries: Vec<String>,
}

/// All keys in `read` must be read at one linearizable serialization point.
/// CAS checks EVERY selected generation and writes all updates atomically.
/// Never retry a mutation on transport errors: its outcome may be unknown.
pub trait AtomicJournalStore {
    fn supports_durable_flush(&self) -> bool {
        false
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError>;
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError>;
}

pub struct ClusterEngine<T, S> {
    store: Arc<Mutex<S>>,
    options: EngineOptions,
    journal_checkpoint_interval: Option<u64>,
    marker: PhantomData<T>,
}
impl<T: LinkReference, S: AtomicJournalStore> ClusterEngine<T, S> {
    pub fn new(store: S, mut options: EngineOptions) -> Result<Self, LinkError> {
        if options.concurrency != ConcurrencyControl::Optimistic
            || options.isolation != IsolationLevel::Serializable
        {
            return Err(error(
                "network sessions require optimistic admission and serializable validation",
            ));
        }
        if options.require_durable_flush && !store.supports_durable_flush() {
            return Err(error("coordinator does not support durable flush"));
        }
        // Local memory journals are private workspaces, never the durability boundary.
        options.require_durable_flush = false;
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            options,
            journal_checkpoint_interval: None,
            marker: PhantomData,
        })
    }
    /// Replace the published journal with retained snapshots every N revisions.
    /// Disabled by default. The atomic store remains the publication boundary.
    pub fn with_journal_checkpoint_interval(mut self, interval: u64) -> Result<Self, LinkError> {
        if interval == 0 {
            return Err(error("journal checkpoint interval must be positive"));
        }
        self.journal_checkpoint_interval = Some(interval);
        Ok(self)
    }
    pub fn begin(&self, keys: &[&str]) -> Result<ClusterTransaction<T, S>, LinkError> {
        let keys: Vec<String> = keys.iter().map(|key| (*key).into()).collect();
        validate_keys(&keys)?;
        let expected = self
            .store
            .lock()
            .map_err(|_| error("coordinator mutex poisoned"))?
            .read(&keys)?;
        if expected.len() != keys.len() {
            return Err(error("incomplete coordinator snapshot"));
        }
        let mut shards = Vec::new();
        for version in &expected {
            let engine = Engine::open(
                MemoryLog {
                    entries: version.entries.clone(),
                },
                self.options.clone(),
            )?;
            let session = engine.begin()?;
            shards.push(ShardSession { engine, session });
        }
        Ok(ClusterTransaction {
            store: self.store.clone(),
            keys,
            expected,
            shards,
            journal_checkpoint_interval: self.journal_checkpoint_interval,
            closed: false,
        })
    }
}

pub(super) fn validate_keys(keys: &[String]) -> Result<(), LinkError> {
    if keys.is_empty()
        || keys.len() > 32
        || keys.iter().enumerate().any(|(index, key)| {
            key.is_empty()
                || key.len() > 128
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                || keys[..index].contains(key)
        })
    {
        return Err(error(
            "select 1..32 distinct shard names using letters, numbers, dot, underscore or dash",
        ));
    }
    Ok(())
}

pub struct ClusterTransaction<T: LinkReference, S: AtomicJournalStore> {
    store: Arc<Mutex<S>>,
    keys: Vec<String>,
    expected: Vec<JournalVersion>,
    shards: Vec<ShardSession<T>>,
    journal_checkpoint_interval: Option<u64>,
    closed: bool,
}
impl<T: LinkReference, S: AtomicJournalStore> ClusterTransaction<T, S> {
    pub fn shard(&mut self, key: &str) -> Result<&mut ShardSession<T>, LinkError> {
        if self.closed {
            return Err(error("cluster transaction is closed"));
        }
        let index = self
            .keys
            .iter()
            .position(|candidate| candidate == key)
            .ok_or_else(|| error("shard was not selected at begin"))?;
        Ok(&mut self.shards[index])
    }
    /// Every attempt ends this transaction. An error after CAS may be unknown;
    /// use a new transaction to discover the authoritative state.
    pub fn commit(&mut self) -> Result<(), LinkError> {
        if self.closed {
            return Err(error("cluster transaction is closed"));
        }
        self.closed = true;
        let mut updates = Vec::new();
        for (shard, expected) in self.shards.iter_mut().zip(&self.expected) {
            shard.session.commit()?;
            let mut entries = shard.engine.with_log(|log| Ok(log.entries.clone()))?;
            // Compare before compaction: a checkpoint can shrink the journal or
            // leave its length unchanged. Read-only groups publish no rewrite.
            if entries.len() == expected.entries.len() {
                updates.push(None);
            } else {
                if let Some(interval) = self.journal_checkpoint_interval {
                    if shard.engine.snapshot()?.revision.is_multiple_of(interval) {
                        entries = shard.engine.export_checkpoint()?;
                    }
                }
                updates.push(Some(entries));
            }
        }
        let outcome = self
            .store
            .lock()
            .map_err(|_| error("coordinator mutex poisoned"))?
            .compare_exchange(&self.keys, &self.expected, &updates);
        match outcome {
            Ok(true) => Ok(()),
            Ok(false) => Err(error("conflict: a selected shard changed since begin")),
            Err(cause) => Err(error(&format!("indeterminate coordinated commit: {cause}; read fresh journals to discover the outcome"))),
        }
    }
    pub fn rollback(&mut self) {
        self.closed = true;
        for shard in &mut self.shards {
            shard.session.rollback();
        }
    }
}
impl<T: LinkReference, S: AtomicJournalStore> Drop for ClusterTransaction<T, S> {
    fn drop(&mut self) {
        self.rollback();
    }
}

/// Standard raw links operations in a selected shard. Commit the containing
/// group explicitly; individual shard flushes cannot publish a partial group.
pub struct ShardSession<T: LinkReference> {
    engine: Engine<T, MemoryLog>,
    pub(super) session: EngineTransaction<T, MemoryLog>,
}
impl<T: LinkReference> ShardSession<T> {
    pub fn get_name(&self, id: T) -> Option<String> {
        self.session.get_name(id)
    }
    pub fn get_by_name(&self, name: &str) -> Option<T> {
        self.session.get_by_name(name)
    }
    pub fn set_name(&mut self, id: T, name: Option<String>) -> Result<(), LinkError> {
        self.session.set_name(id, name)
    }
}
impl<T: LinkReference> LinksStorage<T> for ShardSession<T> {
    fn create_link(&mut self, source: T, target: T) -> Result<T, LinkError> {
        self.session.create_link(source, target)
    }
    fn ensure_link_created(&mut self, index: T) -> Result<T, LinkError> {
        self.session.ensure_link_created(index)
    }
    fn get_link(&self, index: T) -> Option<GenericLink<T>> {
        self.session.get_link(index)
    }
    fn update_link(&mut self, index: T, source: T, target: T) -> Result<GenericLink<T>, LinkError> {
        self.session.update_link(index, source, target)
    }
    fn delete_link(&mut self, index: T) -> Result<GenericLink<T>, LinkError> {
        self.session.delete_link(index)
    }
    fn all_links(&self) -> Vec<GenericLink<T>> {
        self.session.all_links()
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        Err(error("commit the containing cluster transaction"))
    }
}
