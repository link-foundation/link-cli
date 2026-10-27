use super::{invalid, lease::Lease, LsmOptions, LsmTableDescriptor, LsmWalState};
use crate::{
    engine::{
        cluster::{validate_keys, AtomicJournalStore, JournalVersion},
        maintenance::{JournalMaintenanceStore, JournalNamespaceEntry, JournalScanOptions},
        paged::{PageDevice, SortedJournalEntry, SortedJournalTable, SortedTableOptions},
    },
    LinkError, ReplaceableTransitionLogStore,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// Exclusive synchronous namespace. List returns every ordered numeric identity
/// or fails within its bound. Create never reuses a name; open never creates
/// missing files. Successful create/delete persist directory entries. Close
/// aborts staged repair and permits retry after an uncertain result.
pub trait LsmTableNamespace {
    type Device: PageDevice;
    fn supports_durable_flush(&self) -> bool {
        false
    }
    fn list(&mut self, max_entries: usize) -> Result<Vec<u64>, LinkError>;
    fn create(&mut self, id: u64) -> Result<Self::Device, LinkError>;
    fn open(&mut self, id: u64) -> Result<Self::Device, LinkError>;
    fn close(&mut self, device: &mut Self::Device) -> Result<(), LinkError>;
    fn delete(&mut self, id: u64) -> Result<(), LinkError>;
}
#[derive(Clone, Copy, Debug)]
pub struct LsmStoreOptions {
    pub limits: LsmOptions,
    pub max_namespace_entries: usize,
}
impl Default for LsmStoreOptions {
    fn default() -> Self {
        Self {
            limits: LsmOptions::default(),
            max_namespace_entries: 256,
        }
    }
}
#[derive(Clone)]
struct Index {
    generation: u64,
    tombstone: bool,
    bytes: usize,
    owner: u64,
}
struct Plan {
    merge: bool,
    rows: Vec<SortedJournalEntry>,
    stored: usize,
    id: u64,
    reserved: LsmWalState,
    reservation: String,
    published: LsmWalState,
}
/// Bounded physical LSM with synchronous full compaction. Disk payloads are read
/// from actual pages; only metadata is indexed. Owns its WAL and namespace by
/// value, returned by into_parts without closing them. Table leases are closed
/// explicitly; use close to observe failures (Drop is best effort only).
/// Retain shared provider handles if constructor-failure cleanup must be retried.
pub struct PhysicalLsmJournalStore<W: ReplaceableTransitionLogStore, P: LsmTableNamespace> {
    wal: Option<W>,
    provider: Option<P>,
    options: LsmStoreOptions,
    state: LsmWalState,
    index: BTreeMap<String, Index>,
    tables: BTreeMap<u64, SortedJournalTable<Lease<P::Device>>>,
    leases: BTreeMap<u64, Lease<P::Device>>,
    record_count: usize,
    wal_bytes: usize,
    durable: bool,
    table_durable: bool,
    closed: bool,
    fenced: bool,
}
fn cost(row: &SortedJournalEntry) -> usize {
    row.key.len()
        + 8
        + row
            .entries
            .as_ref()
            .map_or(0, |items| items.iter().map(|s| s.len() + 1).sum::<usize>())
}
fn describe(row: &SortedJournalEntry, owner: u64) -> Index {
    Index {
        generation: row.generation,
        tombstone: row.entries.is_none(),
        bytes: cost(row),
        owner,
    }
}
fn wire(row: &SortedJournalEntry) -> serde_json::Value {
    json!([row.key, row.generation.to_string(), row.entries])
}
fn identity<D: PageDevice>(table: &SortedJournalTable<D>) -> String {
    table
        .identity()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
impl<W: ReplaceableTransitionLogStore, P: LsmTableNamespace> PhysicalLsmJournalStore<W, P> {
    pub fn open(wal: W, provider: P, options: LsmStoreOptions) -> Result<Self, LinkError> {
        options.limits.validate()?;
        if !(2..=65_536).contains(&options.max_namespace_entries) || !wal.supports_replacement() {
            return Err(invalid("namespace bounds or nonreplaceable WAL"));
        }
        let table_durable = provider.supports_durable_flush();
        let mut store = Self {
            durable: wal.supports_durable_flush() && table_durable,
            table_durable,
            wal: Some(wal),
            provider: Some(provider),
            options,
            state: LsmWalState::default(),
            index: BTreeMap::new(),
            tables: BTreeMap::new(),
            leases: BTreeMap::new(),
            record_count: 0,
            wal_bytes: 0,
            closed: false,
            fenced: false,
        };
        if let Err(cause) = store.run(Self::recover) {
            let cleanup = store.release();
            store.closed = true;
            if cleanup.is_empty() {
                return Err(cause);
            }
            // Preserve all cleanup errors, rather than dropping constructor leases
            // silently after an exclusive device has been acquired.
            return Err(invalid(&format!(
                "recovery failed: {cause}; lease cleanup: {cleanup:?}"
            )));
        }
        Ok(store)
    }
    fn wal(&mut self) -> &mut W {
        self.wal.as_mut().expect("owned LSM WAL")
    }
    fn provider(&mut self) -> &mut P {
        self.provider.as_mut().expect("owned LSM namespace")
    }
    fn run<T>(
        &mut self,
        action: impl FnOnce(&mut Self) -> Result<T, LinkError>,
    ) -> Result<T, LinkError> {
        if self.closed {
            return Err(invalid("LSM store is closed"));
        }
        if self.fenced {
            return Err(invalid("uncertain LSM owner is fenced; reopen"));
        }
        let result = action(self);
        if result.is_ok() {
            self.fenced = false;
        }
        result
    }
    fn table_options(&self) -> SortedTableOptions {
        SortedTableOptions {
            max_keys: self.options.limits.max_keys,
            max_bytes: 67_108_864,
        }
    }
    fn admit(&self, index: &BTreeMap<String, Index>) -> Result<(), LinkError> {
        if index.len() > self.options.limits.max_keys
            || index.values().map(|row| row.bytes).sum::<usize>() > self.options.limits.max_bytes
        {
            return Err(invalid("retained capacity exceeded"));
        }
        Ok(())
    }
    fn acquire(&mut self, id: u64, create: bool) -> Result<Lease<P::Device>, LinkError> {
        if create {
            self.fenced = true;
        }
        let device = if create {
            self.provider().create(id)?
        } else {
            self.provider().open(id)?
        };
        let lease = Lease::new(device);
        self.leases.insert(id, lease.clone());
        if self.table_durable && !lease.supports_durable_flush() {
            return Err(invalid(
                "durable namespace returned a non-durable table device",
            ));
        }
        Ok(lease)
    }
    fn recover(&mut self) -> Result<(), LinkError> {
        let bound = self.options.max_namespace_entries;
        let names = self.provider().list(bound)?;
        if names.len() > bound
            || names.first() == Some(&0)
            || names.windows(2).any(|p| p[0] >= p[1])
        {
            return Err(invalid("incomplete or unordered namespace"));
        }
        let records = self.wal().read_log_entries()?;
        if records.is_empty() {
            if !names.is_empty() {
                return Err(invalid("uninitialized namespace contains tables"));
            }
            return self.replace(self.state.encode(self.options.limits)?);
        }
        self.state = LsmWalState::recover(&records, self.options.limits)?;
        self.record_count = records.len();
        self.wal_bytes = records.iter().map(String::len).sum();
        let selected: BTreeSet<_> = self.state.manifest.iter().map(|d| d.id).collect();
        if names
            .iter()
            .any(|id| *id > self.state.last_reserved_table_id)
            || selected.iter().any(|id| names.binary_search(id).is_err())
        {
            return Err(invalid("missing selected or unreserved table"));
        }
        let mut seen = BTreeMap::new();
        for descriptor in self.state.manifest.clone() {
            let mut device = self.acquire(descriptor.id, false)?;
            device.deferred = true;
            let mut table = SortedJournalTable::open(device, self.table_options())?;
            if identity(&table) != descriptor.identity
                || table.digest() != descriptor.digest
                || table.len() != descriptor.count
                || table.stored_bytes() != descriptor.stored_bytes
            {
                return Err(invalid("selected table binding changed"));
            }
            let rows = table.entries()?;
            self.tables.insert(descriptor.id, table);
            for row in rows {
                if row.generation > self.state.covered_generation {
                    return Err(invalid("table generation exceeds coverage"));
                }
                let version = (row.key.clone(), row.generation);
                if let Some(previous) = seen.get(&version) {
                    let old = self
                        .tables
                        .get_mut(previous)
                        .expect("seen selected table")
                        .get(&row.key)?;
                    if old.as_ref() != Some(&row) {
                        return Err(invalid("conflicting table versions"));
                    }
                } else {
                    seen.insert(version, descriptor.id);
                }
                if self
                    .index
                    .get(&row.key)
                    .is_none_or(|old| row.generation > old.generation)
                {
                    self.index
                        .insert(row.key.clone(), describe(&row, descriptor.id));
                }
            }
        }
        for row in &self.state.buffer {
            self.index.insert(row.key.clone(), describe(row, 0));
        }
        self.admit(&self.index)?;
        if self
            .index
            .values()
            .map(|row| row.generation)
            .max()
            .unwrap_or(0)
            != self.state.generation
        {
            return Err(invalid("selected data is missing global generation"));
        }
        for mut device in self.leases.values().cloned().collect::<Vec<_>>() {
            self.fenced = true;
            device.complete_recovery()?;
        }
        for id in names {
            if !selected.contains(&id) {
                self.fenced = true;
                self.provider().delete(id)?;
            }
        }
        Ok(())
    }
    fn value(&mut self, key: &str) -> Result<Option<SortedJournalEntry>, LinkError> {
        let Some(item) = self.index.get(key).cloned() else {
            return Ok(None);
        };
        if item.owner == 0 {
            return Ok(self.state.buffer.iter().find(|r| r.key == key).cloned());
        }
        let row = self
            .tables
            .get_mut(&item.owner)
            .expect("indexed selected table")
            .get(key)?;
        if row.as_ref().is_none_or(|r| {
            r.generation != item.generation
                || r.entries.is_none() != item.tombstone
                || cost(r) != item.bytes
        }) {
            return Err(invalid("point lookup changed"));
        }
        Ok(row)
    }
    fn version(&mut self, key: &str) -> Result<JournalVersion, LinkError> {
        Ok(self
            .value(key)?
            .map_or_else(JournalVersion::default, |row| JournalVersion {
                generation: row.generation,
                entries: row.entries.unwrap_or_default(),
            }))
    }
    fn replace(&mut self, record: String) -> Result<(), LinkError> {
        self.fenced = true;
        self.wal()
            .replace_log_entries(std::slice::from_ref(&record))?;
        self.record_count = 1;
        self.wal_bytes = record.len();
        Ok(())
    }
    fn commit(&mut self, mut rows: Vec<SortedJournalEntry>) -> Result<(), LinkError> {
        let generation = self
            .state
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("generation exhausted"))?;
        for row in &mut rows {
            row.generation = generation;
        }
        rows.sort_by(|a, b| a.key.cmp(&b.key));
        let limits = self.options.limits;
        let mut candidate = self.index.clone();
        for row in &rows {
            candidate.insert(row.key.clone(), describe(row, 0));
        }
        self.admit(&candidate)?;
        if rows.len() > limits.buffer_keys
            || rows.iter().map(cost).sum::<usize>() > limits.buffer_bytes
        {
            return Err(invalid("group exceeds buffer admission"));
        }
        let record = json!([
            "put",
            generation.to_string(),
            rows.iter().map(wire).collect::<Vec<_>>()
        ])
        .to_string();
        if limits.max_wal_records < 2 {
            return Err(invalid("writes require a checkpoint and group"));
        }
        let mut merged: BTreeMap<_, _> = self
            .state
            .buffer
            .iter()
            .map(|row| (row.key.clone(), row.clone()))
            .collect();
        for row in &rows {
            merged.insert(row.key.clone(), row.clone());
        }
        let plan = if merged.len() > limits.buffer_keys
            || merged.values().map(cost).sum::<usize>() > limits.buffer_bytes
        {
            self.prepare(false)?
        } else {
            None
        };
        let mut basis = plan
            .as_ref()
            .map_or_else(|| self.state.clone(), |p| p.published.clone());
        if plan.is_some() {
            merged = rows
                .iter()
                .map(|row| (row.key.clone(), row.clone()))
                .collect();
        }
        if basis.encode(limits)?.len() + record.len() > limits.max_wal_bytes {
            return Err(invalid(
                "checkpoint and following group exceed WAL admission",
            ));
        }
        basis.generation = generation;
        basis.buffer = merged.into_values().collect();
        basis.encode(limits)?;
        if let Some(plan) = plan {
            self.publish(plan)?;
        }
        if self.record_count + 1 > limits.max_wal_records
            || self.wal_bytes + record.len() > limits.max_wal_bytes
        {
            self.replace(self.state.encode(limits)?)?;
        }
        self.fenced = true;
        self.wal().append_log_entry(&record)?;
        self.wal().flush_log()?;
        self.state.generation = generation;
        self.state.buffer = basis.buffer;
        self.record_count += 1;
        self.wal_bytes += record.len();
        for row in rows {
            self.index.insert(row.key.clone(), describe(&row, 0));
        }
        Ok(())
    }
    fn prepare(&mut self, compact: bool) -> Result<Option<Plan>, LinkError> {
        if self.state.buffer.is_empty() && (!compact || self.state.manifest.len() <= 1) {
            return Ok(None);
        }
        let id = self
            .state
            .last_reserved_table_id
            .checked_add(1)
            .ok_or_else(|| invalid("table identity exhausted"))?;
        let merge = compact || self.state.manifest.len() >= self.options.limits.max_runs;
        let rows = if merge {
            let keys: Vec<_> = self.index.keys().cloned().collect();
            keys.iter()
                .map(|key| self.value(key).map(|row| row.expect("indexed row")))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            self.state.buffer.clone()
        };
        let stored = 15
            + rows
                .iter()
                .map(|row| wire(row).to_string().len())
                .sum::<usize>();
        if stored > 67_108_864 || self.state.manifest.len() + 1 > self.options.max_namespace_entries
        {
            return Err(invalid("table or namespace admission"));
        }
        let mut reserved = self.state.clone();
        reserved.last_reserved_table_id = id;
        let reservation = reserved.encode(self.options.limits)?;
        let mut published = reserved.clone();
        if merge {
            published.manifest.clear();
        }
        published.manifest.push(LsmTableDescriptor {
            id,
            identity: "0".repeat(32),
            digest: u64::MAX,
            count: rows.len(),
            stored_bytes: stored,
        });
        published.covered_generation = published.generation;
        published.buffer.clear();
        published.encode(self.options.limits)?;
        Ok(Some(Plan {
            merge,
            rows,
            stored,
            id,
            reserved,
            reservation,
            published,
        }))
    }
    fn publish(&mut self, mut plan: Plan) -> Result<(), LinkError> {
        self.replace(plan.reservation)?;
        self.state = plan.reserved;
        let device = self.acquire(plan.id, true)?;
        self.fenced = true;
        let table = SortedJournalTable::create(device, &plan.rows, self.table_options())?;
        if table.len() != plan.rows.len() || table.stored_bytes() != plan.stored {
            return Err(invalid("prepared table bounds changed"));
        }
        let descriptor = LsmTableDescriptor {
            id: plan.id,
            identity: identity(&table),
            digest: table.digest(),
            count: table.len(),
            stored_bytes: table.stored_bytes(),
        };
        self.tables.insert(plan.id, table);
        *plan
            .published
            .manifest
            .last_mut()
            .expect("prepared descriptor") = descriptor;
        self.replace(plan.published.encode(self.options.limits)?)?;
        let obsolete = if plan.merge {
            self.state.manifest.clone()
        } else {
            vec![]
        };
        self.state = plan.published;
        for row in plan.rows {
            self.index.insert(row.key.clone(), describe(&row, plan.id));
        }
        for old in obsolete {
            self.fenced = true;
            self.release_one(old.id)?;
            self.tables.remove(&old.id);
            self.provider().delete(old.id)?;
        }
        Ok(())
    }
    pub fn flush(&mut self) -> Result<(), LinkError> {
        self.run(|s| {
            if let Some(plan) = s.prepare(false)? {
                s.publish(plan)?;
            }
            Ok(())
        })
    }
    pub fn compact(&mut self) -> Result<(), LinkError> {
        self.run(|s| {
            if let Some(plan) = s.prepare(true)? {
                s.publish(plan)?;
            }
            Ok(())
        })
    }
    fn release_one(&mut self, id: u64) -> Result<(), LinkError> {
        let lease = self.leases.get(&id).expect("owned table lease").clone();
        {
            let mut device = lease
                .device
                .lock()
                .map_err(|_| invalid("table lease mutex poisoned"))?;
            self.provider().close(&mut *device)?;
        }
        self.leases.remove(&id);
        Ok(())
    }
    fn release(&mut self) -> Vec<LinkError> {
        let ids: Vec<_> = self.leases.keys().rev().copied().collect();
        ids.into_iter()
            .filter_map(|id| self.release_one(id).err())
            .collect()
    }
    pub fn close(&mut self) -> Result<(), LinkError> {
        self.closed = true;
        let errors = self.release();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(invalid(&format!(
                "cleanup failed; ownership remains fenced: {errors:?}"
            )))
        }
    }
    pub fn into_parts(mut self) -> Result<(W, P), LinkError> {
        self.close()?;
        Ok((
            self.wal.take().expect("owned WAL"),
            self.provider.take().expect("owned namespace"),
        ))
    }
}
impl<W: ReplaceableTransitionLogStore, P: LsmTableNamespace> Drop
    for PhysicalLsmJournalStore<W, P>
{
    fn drop(&mut self) {
        let _ = self.release();
    }
}
impl<W: ReplaceableTransitionLogStore, P: LsmTableNamespace> AtomicJournalStore
    for PhysicalLsmJournalStore<W, P>
{
    fn supports_durable_flush(&self) -> bool {
        self.durable
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        self.run(|s| {
            validate_keys(keys)?;
            keys.iter().map(|key| s.version(key)).collect()
        })
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        self.run(|s| {
            validate_keys(keys)?;
            if keys.len() != expected.len() || keys.len() != updates.len() {
                return Err(invalid("group array lengths"));
            }
            if keys.iter().zip(expected).any(|(key, guard)| {
                s.index.get(key).map_or(0, |r| r.generation) != guard.generation
            }) {
                return Ok(false);
            }
            let mut changes = Vec::new();
            for (key, update) in keys.iter().zip(updates) {
                if let Some(entries) = update {
                    if s.value(key)?.as_ref().and_then(|r| r.entries.as_ref()) != Some(entries) {
                        changes.push(SortedJournalEntry {
                            key: key.clone(),
                            generation: 0,
                            entries: Some(entries.clone()),
                        });
                    }
                }
            }
            if !changes.is_empty() {
                s.commit(changes)?;
            }
            Ok(true)
        })
    }
}
impl<W: ReplaceableTransitionLogStore, P: LsmTableNamespace> JournalMaintenanceStore
    for PhysicalLsmJournalStore<W, P>
{
    fn scan_namespace(
        &mut self,
        options: JournalScanOptions,
    ) -> Result<Vec<JournalNamespaceEntry>, LinkError> {
        self.run(|s| {
            if options.max_keys == 0
                || options.max_bytes == 0
                || !(1..=256).contains(&options.page_keys)
            {
                return Err(invalid("namespace scan bounds"));
            }
            let keys: Vec<_> = s
                .index
                .iter()
                .filter(|(_, row)| !row.tombstone)
                .map(|(key, row)| (key.clone(), row.generation))
                .collect();
            let mut result = Vec::new();
            let mut bytes = 0;
            for (key, generation) in keys {
                let version = if options.keys_only {
                    JournalVersion {
                        generation,
                        entries: vec![],
                    }
                } else {
                    s.version(&key)?
                };
                bytes += key.len()
                    + 8
                    + version
                        .entries
                        .iter()
                        .map(|entry| entry.len() + 1)
                        .sum::<usize>();
                if result.len() == options.max_keys || bytes > options.max_bytes {
                    return Err(invalid("namespace scan admission"));
                }
                result.push(JournalNamespaceEntry { key, version });
            }
            Ok(result)
        })
    }
    fn compare_delete(&mut self, key: &str, expected_generation: u64) -> Result<bool, LinkError> {
        self.run(|s| {
            validate_keys(&[key.to_owned()])?;
            if expected_generation == 0 {
                return Err(invalid("positive delete generation"));
            }
            if s.index
                .get(key)
                .is_none_or(|row| row.tombstone || row.generation != expected_generation)
            {
                return Ok(false);
            }
            s.commit(vec![SortedJournalEntry {
                key: key.to_owned(),
                generation: 0,
                entries: None,
            }])?;
            Ok(true)
        })
    }
}
