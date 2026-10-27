//! Selected physical LSM components. WAL semantics perform no storage I/O;
//! replay alone does not validate the physical tables selected by a manifest.
use super::paged::SortedJournalEntry;
use crate::LinkError;
use serde_json::{json, Value};
use std::collections::BTreeMap;
mod lease;
mod store;
pub use store::{LsmStoreOptions, LsmTableNamespace, PhysicalLsmJournalStore};

#[derive(Clone, Copy, Debug)]
pub struct LsmOptions {
    pub max_keys: usize,
    pub max_bytes: usize,
    pub buffer_keys: usize,
    pub buffer_bytes: usize,
    pub max_runs: usize,
    pub max_wal_records: usize,
    pub max_wal_bytes: usize,
}
impl Default for LsmOptions {
    fn default() -> Self {
        Self {
            max_keys: 4096,
            max_bytes: 8_388_608,
            buffer_keys: 64,
            buffer_bytes: 1_048_576,
            max_runs: 4,
            max_wal_records: 1024,
            max_wal_bytes: 16_777_216,
        }
    }
}
impl LsmOptions {
    pub fn validate(self) -> Result<(), LinkError> {
        for (value, maximum) in [
            (self.max_keys, 65_536),
            (self.max_bytes, 8_388_608),
            (self.buffer_keys, 65_536),
            (self.buffer_bytes, 8_388_608),
            (self.max_runs, 64),
            (self.max_wal_records, 65_536),
            (self.max_wal_bytes, 67_108_864),
        ] {
            if value == 0 || value > maximum {
                return Err(invalid("admission bounds"));
            }
        }
        if self.buffer_keys > self.max_keys || self.buffer_bytes > self.max_bytes {
            return Err(invalid("buffer exceeds total admission"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LsmTableDescriptor {
    pub id: u64,
    pub identity: String,
    pub digest: u64,
    pub count: usize,
    pub stored_bytes: usize,
}
/// Owned, transient replay result. A physical store must additionally validate
/// every selected table and combined logical capacity before exposing data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LsmWalState {
    pub generation: u64,
    pub covered_generation: u64,
    pub last_reserved_table_id: u64,
    pub manifest: Vec<LsmTableDescriptor>,
    pub buffer: Vec<SortedJournalEntry>,
}
impl LsmWalState {
    pub fn recover(records: &[String], options: LsmOptions) -> Result<Self, LinkError> {
        options.validate()?;
        if records.is_empty() || records.len() > options.max_wal_records {
            return Err(invalid("record count"));
        }
        let mut bytes = 0usize;
        for record in records {
            bytes = bytes
                .checked_add(record.len())
                .ok_or_else(|| invalid("WAL bytes"))?;
            if record.contains(['\r', '\n']) || bytes > options.max_wal_bytes {
                return Err(invalid("physical record or WAL bytes"));
            }
        }
        let checkpoint = parse(&records[0])?;
        let checkpoint = array(&checkpoint, 6)?;
        if checkpoint[0] != "native-lsm1" {
            return Err(invalid("checkpoint format"));
        }
        let mut state = Self {
            generation: counter(&checkpoint[1], false)?,
            covered_generation: counter(&checkpoint[2], false)?,
            last_reserved_table_id: counter(&checkpoint[3], false)?,
            ..Self::default()
        };
        let manifest = checkpoint[4]
            .as_array()
            .ok_or_else(|| invalid("manifest array"))?;
        if state.covered_generation > state.generation || manifest.len() > options.max_runs {
            return Err(invalid("manifest bounds"));
        }
        let mut previous = 0;
        for item in manifest {
            let item = array(item, 5)?;
            let id = counter(&item[0], true)?;
            if id <= previous || id > state.last_reserved_table_id {
                return Err(invalid("table ID ordering or reservation"));
            }
            previous = id;
            let identity = item[1].as_str().ok_or_else(|| invalid("table identity"))?;
            if identity.len() != 32
                || !identity
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(invalid("native table identity"));
            }
            let count = counter(&item[3], false)?;
            let stored_bytes = counter(&item[4], false)?;
            if count > options.max_keys as u64 || !(15..=67_108_864).contains(&stored_bytes) {
                return Err(invalid("table admission"));
            }
            state.manifest.push(LsmTableDescriptor {
                id,
                identity: identity.to_owned(),
                digest: counter(&item[2], false)?,
                count: count as usize,
                stored_bytes: stored_bytes as usize,
            });
        }
        if (state.covered_generation != 0) == state.manifest.is_empty() {
            return Err(invalid("covered table presence"));
        }
        state.buffer = rows(
            &checkpoint[5],
            state.covered_generation,
            state.generation,
            options.buffer_keys,
            options.buffer_bytes,
        )?;
        if state
            .buffer
            .iter()
            .map(|row| row.generation)
            .max()
            .unwrap_or(state.covered_generation)
            != state.generation
        {
            return Err(invalid("missing latest generation"));
        }
        for record in &records[1..] {
            let parsed = parse(record)?;
            let record = array(&parsed, 3)?;
            if record[0] != "put" {
                return Err(invalid("group format"));
            }
            let next = counter(&record[1], true)?;
            if state.generation.checked_add(1) != Some(next) {
                return Err(invalid("group sequence"));
            }
            let changes = rows(&record[2], state.generation, next, 32, options.buffer_bytes)?;
            if changes.is_empty() {
                return Err(invalid("empty group"));
            }
            let mut merged: BTreeMap<_, _> = state
                .buffer
                .into_iter()
                .map(|row| (row.key.clone(), row))
                .collect();
            for row in changes {
                merged.insert(row.key.clone(), row);
            }
            state.buffer = merged.into_values().collect();
            if state.buffer.len() > options.buffer_keys
                || logical_bytes(&state.buffer)? > options.buffer_bytes
            {
                return Err(invalid("replay buffer admission"));
            }
            state.generation = next;
        }
        Ok(state)
    }
    /// Revalidates public fields, including admission, before returning a record.
    pub fn encode(&self, options: LsmOptions) -> Result<String, LinkError> {
        let manifest: Vec<_> = self
            .manifest
            .iter()
            .map(|item| {
                json!([
                    item.id.to_string(),
                    item.identity,
                    item.digest.to_string(),
                    item.count.to_string(),
                    item.stored_bytes.to_string()
                ])
            })
            .collect();
        let buffer: Vec<_> = self
            .buffer
            .iter()
            .map(|row| json!([row.key, row.generation.to_string(), row.entries]))
            .collect();
        let record = json!([
            "native-lsm1",
            self.generation.to_string(),
            self.covered_generation.to_string(),
            self.last_reserved_table_id.to_string(),
            manifest,
            buffer
        ])
        .to_string();
        Self::recover(std::slice::from_ref(&record), options)?;
        Ok(record)
    }
}
fn parse(record: &str) -> Result<Value, LinkError> {
    serde_json::from_str(record).map_err(|_| invalid("JSON"))
}
fn array(value: &Value, length: usize) -> Result<&[Value], LinkError> {
    value
        .as_array()
        .filter(|items| items.len() == length)
        .map(Vec::as_slice)
        .ok_or_else(|| invalid("record shape"))
}
fn counter(value: &Value, positive: bool) -> Result<u64, LinkError> {
    let text = value.as_str().ok_or_else(|| invalid("counter type"))?;
    if text.is_empty() || text.len() > 20 {
        return Err(invalid("canonical u64"));
    }
    let result = text.parse::<u64>().map_err(|_| invalid("u64 range"))?;
    if result.to_string() != text || (positive && result == 0) {
        return Err(invalid("canonical u64"));
    }
    Ok(result)
}
fn rows(
    value: &Value,
    lower: u64,
    upper: u64,
    max_keys: usize,
    max_bytes: usize,
) -> Result<Vec<SortedJournalEntry>, LinkError> {
    let input = value
        .as_array()
        .filter(|rows| rows.len() <= max_keys)
        .ok_or_else(|| invalid("row count"))?;
    let mut result = Vec::with_capacity(input.len());
    let mut previous = "";
    let mut bytes = 0usize;
    for item in input {
        let item = array(item, 3)?;
        let key = item[0].as_str().ok_or_else(|| invalid("row key"))?;
        if key.is_empty()
            || key.len() > 128
            || key <= previous
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            return Err(invalid("row key ordering"));
        }
        previous = key;
        let generation = counter(&item[1], true)?;
        if generation <= lower || generation > upper {
            return Err(invalid("row generation"));
        }
        let entries = if item[2].is_null() {
            None
        } else {
            Some(
                item[2]
                    .as_array()
                    .ok_or_else(|| invalid("row payload"))?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| invalid("record string"))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )
        };
        let row = SortedJournalEntry {
            key: key.to_owned(),
            generation,
            entries,
        };
        bytes = bytes
            .checked_add(logical_bytes(std::slice::from_ref(&row))?)
            .ok_or_else(|| invalid("buffer bytes"))?;
        if bytes > max_bytes {
            return Err(invalid("buffer bytes"));
        }
        result.push(row);
    }
    Ok(result)
}
fn logical_bytes(rows: &[SortedJournalEntry]) -> Result<usize, LinkError> {
    let mut bytes = 0usize;
    for row in rows {
        bytes = bytes
            .checked_add(row.key.len() + 8)
            .ok_or_else(|| invalid("logical bytes"))?;
        if let Some(entries) = &row.entries {
            for entry in entries {
                bytes = bytes
                    .checked_add(entry.len())
                    .and_then(|n| n.checked_add(1))
                    .ok_or_else(|| invalid("logical bytes"))?;
            }
        }
    }
    Ok(bytes)
}
fn invalid(message: &str) -> LinkError {
    LinkError::StorageError(format!("invalid LSM WAL: {message}"))
}
