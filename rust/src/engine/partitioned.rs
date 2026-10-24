//! Immutable journal objects on independent data stores; only small pointers
//! participate in the catalog's atomic transaction. Never mutate/delete objects.
use super::{
    cluster::{validate_keys, AtomicJournalStore, JournalVersion},
    error,
};
use crate::LinkError;
use std::collections::BTreeMap;

pub struct PartitionedJournalStore<C, D> {
    catalog: C,
    partitions: BTreeMap<String, D>,
    route: Box<dyn Fn(&str) -> String + Send + Sync>,
}
impl<C: AtomicJournalStore, D: AtomicJournalStore> PartitionedJournalStore<C, D> {
    pub fn new(
        catalog: C,
        partitions: BTreeMap<String, D>,
        route: impl Fn(&str) -> String + Send + Sync + 'static,
    ) -> Result<Self, LinkError> {
        if partitions.is_empty() {
            return Err(error("provide named atomic data stores"));
        }
        for key in partitions.keys() {
            validate_keys(std::slice::from_ref(key))?;
        }
        Ok(Self {
            catalog,
            partitions,
            route: Box::new(route),
        })
    }
}
// FNV is an index, not authentication. Full contents are compared on creation;
// a collision fails safely rather than overwriting or reusing different data.
fn address(records: &[String]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    let mut add = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    };
    add(b"blob1|");
    for record in records {
        add(format!("{}:", record.len()).as_bytes());
        add(record.as_bytes());
    }
    format!("blob1-{hash:016x}")
}
fn read_one<S: AtomicJournalStore>(store: &mut S, key: &str) -> Result<JournalVersion, LinkError> {
    let mut values = store.read(&[key.into()])?;
    if values.len() != 1 {
        return Err(error("invalid immutable object snapshot"));
    }
    Ok(values.remove(0))
}
fn pointer(parts: &[String]) -> Option<(&str, &str, u64)> {
    if parts.len() != 4 || parts[0] != "blob1" {
        return None;
    }
    let key = &parts[2];
    if key.len() != 22
        || !key.starts_with("blob1-")
        || !key.as_bytes()[6..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return None;
    }
    let generation: u64 = parts[3].parse().ok()?;
    (generation > 0 && generation.to_string() == parts[3]).then_some((
        parts[1].as_str(),
        key.as_str(),
        generation,
    ))
}
impl<C: AtomicJournalStore, D: AtomicJournalStore> AtomicJournalStore
    for PartitionedJournalStore<C, D>
{
    fn supports_durable_flush(&self) -> bool {
        self.catalog.supports_durable_flush()
            && self
                .partitions
                .values()
                .all(AtomicJournalStore::supports_durable_flush)
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        validate_keys(keys)?;
        let pointers = self.catalog.read(keys)?;
        if pointers.len() != keys.len() {
            return Err(error("incomplete catalog snapshot"));
        }
        let mut result = Vec::new();
        for value in pointers {
            if value.generation == 0 && value.entries.is_empty() {
                result.push(value);
                continue;
            }
            let (partition, key, generation) = pointer(&value.entries)
                .filter(|_| value.generation > 0)
                .ok_or_else(|| error("invalid catalog pointer"))?;
            let store = self
                .partitions
                .get_mut(partition)
                .ok_or_else(|| error("invalid catalog pointer partition"))?;
            let object = read_one(store, key)?;
            if object.generation != generation || address(&object.entries) != key {
                return Err(error("missing or modified immutable object"));
            }
            result.push(JournalVersion {
                generation: value.generation,
                entries: object.entries,
            });
        }
        Ok(result)
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        validate_keys(keys)?;
        if expected.len() != keys.len() || updates.len() != keys.len() {
            return Err(error("expected generations and updates must match shards"));
        }
        // Route and validate every update before any external staging operation.
        let routes: Vec<_> = keys
            .iter()
            .zip(updates)
            .map(|(key, update)| {
                update
                    .as_ref()
                    .map(|_| {
                        let partition = (self.route)(key);
                        if !self.partitions.contains_key(&partition) {
                            return Err(error("route must select a configured partition"));
                        }
                        Ok(partition)
                    })
                    .transpose()
            })
            .collect::<Result<_, _>>()?;
        let mut pointers = Vec::new();
        for (update, partition) in updates.iter().zip(routes) {
            let Some(data) = update else {
                pointers.push(None);
                continue;
            };
            let partition = partition.expect("validated route for every update");
            let key = address(data);
            let store = self
                .partitions
                .get_mut(&partition)
                .expect("validated partition");
            let mut object = read_one(store, &key)?;
            if object.generation == 0 {
                // A lost response leaves an orphan. Never replay a mutation.
                store.compare_exchange(
                    std::slice::from_ref(&key),
                    &[object],
                    &[Some(data.clone())],
                )?;
                object = read_one(store, &key)?;
            }
            if object.generation == 0 || object.entries != *data {
                return Err(error(
                    "immutable object collision or inconsistent data store",
                ));
            }
            pointers.push(Some(vec![
                "blob1".into(),
                partition,
                key,
                object.generation.to_string(),
            ]));
        }
        self.catalog.compare_exchange(keys, expected, &pointers)
    }
}
