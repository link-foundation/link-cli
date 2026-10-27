//! Selected bounded local batches; remote leaves do not retain group atomicity.
use super::{
    causal::{CausalPayload, CausalVersion},
    causal_replica::{CausalReplica, CausalReplicaOptions, CausalSnapshot},
    cluster::{AtomicJournalStore, JournalVersion},
};
use crate::LinkError;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn invalid() -> LinkError {
    LinkError::InvalidFormat("invalid causal batch state".into())
}
fn identity(value: &str) -> Result<u128, LinkError> {
    if value.len() != 32
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    u128::from_str_radix(value, 16)
        .ok()
        .filter(|v| *v != 0)
        .ok_or_else(invalid)
}
fn number(value: &Value) -> Result<u64, LinkError> {
    let text = value.as_str().ok_or_else(invalid)?;
    text.parse::<u64>()
        .ok()
        .filter(|v| v.to_string() == text)
        .ok_or_else(invalid)
}
fn raw(value: &CausalVersion) -> Result<Value, LinkError> {
    serde_json::from_str(&value.encode()?).map_err(|_| invalid())
}
fn range(token: u128, count: usize) -> Result<(), LinkError> {
    token
        .checked_add(count.saturating_sub(1) as u128)
        .ok_or_else(invalid)?;
    Ok(())
}
#[derive(Clone, Debug)]
pub struct CausalBatchEdit {
    pub address: u64,
    pub observed: Vec<CausalVersion>,
    pub payload: Option<CausalPayload>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalBatchSnapshot {
    pub generation: u64,
    pub author: String,
    pub last_counter: u64,
    pub versions: Vec<CausalVersion>,
}
fn normalize(edits: &[CausalBatchEdit]) -> Result<Vec<Value>, LinkError> {
    if edits.len() > 64 {
        return Err(invalid());
    }
    let mut previous = 0;
    edits
        .iter()
        .map(|edit| {
            if edit.address <= previous || edit.observed.len() > 64 {
                return Err(invalid());
            }
            previous = edit.address;
            let context = CausalVersion::merge(&edit.observed, &[])?;
            let dummy = CausalVersion::advance(
                edit.address,
                "00000000000000000000000000000001",
                0,
                &[],
                edit.payload.clone(),
            )?;
            Ok(json!([
                edit.address.to_string(),
                context.iter().map(raw).collect::<Result<Vec<_>, _>>()?,
                raw(&dummy)?[4]
            ]))
        })
        .collect()
}
fn parse_edits(value: &Value, author: &str) -> Result<Vec<CausalBatchEdit>, LinkError> {
    let edits = value
        .as_array()
        .filter(|v| v.len() <= 64)
        .ok_or_else(invalid)?;
    edits
        .iter()
        .map(|value| {
            let value = value
                .as_array()
                .filter(|v| v.len() == 3)
                .ok_or_else(invalid)?;
            let address = number(&value[0])?;
            let observed = value[1]
                .as_array()
                .filter(|v| v.len() <= 64)
                .ok_or_else(invalid)?
                .iter()
                .map(|v| CausalVersion::decode(&v.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            let dummy = CausalVersion::decode(
                &json!([
                    "causal-version1",
                    value[0],
                    [author, "1"],
                    [[author, "1"]],
                    value[2]
                ])
                .to_string(),
            )?;
            Ok(CausalBatchEdit {
                address,
                observed,
                payload: dummy.payload,
            })
        })
        .collect()
}
struct StagingStore(JournalVersion);
impl AtomicJournalStore for StagingStore {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn read(&mut self, _: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        Ok(vec![self.0.clone()])
    }
    fn compare_exchange(
        &mut self,
        _: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        self.0 = JournalVersion {
            generation: expected[0].generation + 1,
            entries: updates[0].clone().ok_or_else(invalid)?,
        };
        Ok(true)
    }
}
struct Receipt {
    request: Value,
    versions: Vec<CausalVersion>,
}
struct Loaded {
    expected: Vec<JournalVersion>,
    owner: CausalReplica<StagingStore>,
    snapshot: CausalSnapshot,
    receipts: BTreeMap<String, Receipt>,
}
/// The caller owns the coherent physical store. Reserve distinct owner/receipt
/// keys and disjoint child-token ranges for one uniquely fenced author lineage.
pub struct CausalBatch<S> {
    store: S,
    keys: Vec<String>,
    author: String,
    options: CausalReplicaOptions,
}
impl<S: AtomicJournalStore> CausalBatch<S> {
    pub fn new(
        store: S,
        owner_key: &str,
        receipt_key: &str,
        author: &str,
        options: CausalReplicaOptions,
    ) -> Result<Self, LinkError> {
        let owner = CausalReplica::new(store, owner_key, author, options)?;
        if receipt_key.is_empty()
            || receipt_key.len() > 128
            || receipt_key == owner_key
            || !receipt_key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            return Err(invalid());
        }
        Ok(Self {
            store: owner.into_store(),
            keys: vec![owner_key.into(), receipt_key.into()],
            author: author.into(),
            options,
        })
    }
    pub fn into_store(self) -> S {
        self.store
    }
    fn admit(&self, entries: &[String]) -> Result<(), LinkError> {
        if entries.len() > self.options.max_entries {
            return Err(invalid());
        }
        let mut bytes = 0usize;
        for entry in entries {
            bytes = bytes.checked_add(entry.len()).ok_or_else(invalid)?;
            if entry.len() > 1_048_576 || bytes > self.options.max_bytes {
                return Err(invalid());
            }
            let (mut depth, mut separators, mut quoted) = (0i32, 0usize, false);
            for item in entry.bytes() {
                if !(b' '..=b'~').contains(&item) || item == b'\\' {
                    return Err(invalid());
                }
                if item == b'"' {
                    quoted = !quoted;
                    continue;
                }
                if quoted {
                    continue;
                }
                match item {
                    b'{' | b'}' => return Err(invalid()),
                    b'[' => {
                        depth += 1;
                        if depth > 7 {
                            return Err(invalid());
                        }
                    }
                    b']' => {
                        depth -= 1;
                        if depth < 0 {
                            return Err(invalid());
                        }
                    }
                    b',' => {
                        separators += 1;
                        if separators > 32768 {
                            return Err(invalid());
                        }
                    }
                    _ => {}
                }
            }
            if quoted || depth != 0 {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn load(&mut self) -> Result<Loaded, LinkError> {
        let expected = self.store.read(&self.keys)?;
        if expected.len() != 2 {
            return Err(invalid());
        }
        for value in &expected {
            self.admit(&value.entries)?;
        }
        let staging = StagingStore(JournalVersion {
            generation: u64::from(expected[0].generation != 0),
            entries: expected[0].entries.clone(),
        });
        let mut owner = CausalReplica::new(staging, &self.keys[0], &self.author, self.options)?;
        let snapshot = owner.snapshot()?;
        let mut writes = BTreeMap::new();
        for text in expected[0].entries.iter().skip(1) {
            let entry: Value = serde_json::from_str(text).map_err(|_| invalid())?;
            if entry[0] == "causal-write1" {
                let value = CausalVersion::decode(&entry[3].to_string())?;
                let observed = entry[2]
                    .as_array()
                    .ok_or_else(invalid)?
                    .iter()
                    .map(|v| CausalVersion::decode(&v.to_string()))
                    .collect::<Result<Vec<_>, _>>()?;
                let encoded = normalize(&[CausalBatchEdit {
                    address: value.address,
                    observed,
                    payload: value.payload.clone(),
                }])?
                .remove(0);
                writes.insert(
                    entry[1].as_str().ok_or_else(invalid)?.to_owned(),
                    (encoded, value),
                );
            }
        }
        let mut receipts = BTreeMap::new();
        let mut used = BTreeSet::new();
        let metadata = &expected[1];
        if metadata.generation == 0 {
            if !metadata.entries.is_empty() {
                return Err(invalid());
            }
        } else {
            if expected[0].generation == 0
                || metadata.entries.first()
                    != Some(&json!(["causal-batches1", self.keys[0], self.author]).to_string())
            {
                return Err(invalid());
            }
            for text in metadata.entries.iter().skip(1) {
                let value: Value = serde_json::from_str(text).map_err(|_| invalid())?;
                let canonical = value.to_string();
                if canonical != *text {
                    return Err(invalid());
                }
                let entry = value
                    .as_array()
                    .filter(|v| v.len() == 5 && v[0] == "causal-batch1")
                    .ok_or_else(invalid)?;
                let token = entry[1].as_str().ok_or_else(invalid)?;
                let base = identity(token)?;
                let generation = number(&entry[2])?;
                let start = number(&entry[3])?;
                if receipts.contains_key(token) || start > snapshot.last_counter {
                    return Err(invalid());
                }
                let edits = parse_edits(&entry[4], &self.author)?;
                let encoded = normalize(&edits)?;
                range(base, edits.len())?;
                if json!(encoded) != entry[4] {
                    return Err(invalid());
                }
                let versions = encoded
                    .iter()
                    .enumerate()
                    .map(|(index, edit)| {
                        let child = format!("{:032x}", base + index as u128);
                        let (prior, version) = writes.get(&child).ok_or_else(invalid)?;
                        if !used.insert(child)
                            || version.dot.counter
                                != start.checked_add(index as u64 + 1).ok_or_else(invalid)?
                            || prior != edit
                        {
                            return Err(invalid());
                        }
                        Ok(version.clone())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                receipts.insert(
                    token.into(),
                    Receipt {
                        request: json!([generation.to_string(), encoded]),
                        versions,
                    },
                );
            }
        }
        Ok(Loaded {
            expected,
            owner,
            snapshot,
            receipts,
        })
    }
    pub fn snapshot(&mut self) -> Result<CausalBatchSnapshot, LinkError> {
        let loaded = self.load()?;
        Ok(CausalBatchSnapshot {
            generation: loaded.expected[0].generation,
            author: loaded.snapshot.author,
            last_counter: loaded.snapshot.last_counter,
            versions: loaded.snapshot.versions,
        })
    }
    pub fn write(
        &mut self,
        token: &str,
        generation: u64,
        edits: &[CausalBatchEdit],
    ) -> Result<Vec<CausalVersion>, LinkError> {
        let base = identity(token)?;
        let encoded = normalize(edits)?;
        range(base, edits.len())?;
        let request = json!([generation.to_string(), encoded]);
        let mut loaded = self.load()?;
        let receipt = loaded.receipts.get(token);
        if let Some(receipt) = receipt {
            if receipt.request != request {
                return Err(invalid());
            }
        } else if loaded.expected[0].generation != generation {
            return Err(LinkError::Transaction(
                "conflict: stale causal batch base".into(),
            ));
        }
        let mut result = Vec::with_capacity(edits.len());
        for (index, edit) in edits.iter().enumerate() {
            let value = loaded.owner.write(
                &format!("{:032x}", base + index as u128),
                edit.address,
                &edit.observed,
                edit.payload.clone(),
            )?;
            let counter = match receipt {
                Some(receipt) => receipt.versions[index].dot.counter,
                None => loaded
                    .snapshot
                    .last_counter
                    .checked_add(index as u64 + 1)
                    .ok_or_else(invalid)?,
            };
            if value.dot.counter != counter {
                return Err(LinkError::InvalidFormat(
                    "causal batch child-token collision".into(),
                ));
            }
            result.push(value);
        }
        if receipt.is_some() {
            return Ok(result);
        }
        let mut owner_entries = loaded.owner.into_store().0.entries;
        if owner_entries.is_empty() {
            owner_entries.push(json!(["causal-replica1", self.author]).to_string());
        }
        let mut entries = loaded.expected[1].entries.clone();
        if entries.is_empty() {
            entries.push(json!(["causal-batches1", self.keys[0], self.author]).to_string());
        }
        entries.push(
            json!([
                "causal-batch1",
                token,
                generation.to_string(),
                loaded.snapshot.last_counter.to_string(),
                encoded
            ])
            .to_string(),
        );
        self.admit(&entries)?;
        match self.store.compare_exchange(&self.keys, &loaded.expected, &[Some(owner_entries), Some(entries)]) {
            Ok(true) => Ok(result),
            Ok(false) => Err(LinkError::Transaction("conflict: causal batch state changed".into())),
            Err(cause) => Err(LinkError::Transaction(format!("indeterminate causal batch publication; reread and retry original request: {cause}"))),
        }
    }
}
