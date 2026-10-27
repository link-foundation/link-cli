//! Immutable journal objects on independent data stores; only small pointers
//! participate in the catalog's atomic transaction. Never mutate/delete objects.
use super::{
    cluster::{validate_keys, AtomicJournalStore, JournalVersion},
    error,
};
use crate::LinkError;
use std::collections::BTreeMap;
#[path = "partitioned/chunks.rs"]
mod chunks;
pub use chunks::JournalChunkOptions;

pub struct PartitionedJournalStore<C, D> {
    catalog: C,
    partitions: BTreeMap<String, D>,
    route: Box<dyn Fn(&str) -> String + Send + Sync>,
    cache: Option<super::object_cache::ObjectCache>,
    chunks: Option<JournalChunkOptions>,
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
            cache: None,
            chunks: None,
        })
    }

    /// Cache verified immutable objects within a UTF-8 payload budget. Zero
    /// disables caching. Catalog reads and publication checks always stay fresh.
    /// Budget counts one byte per object/record plus record UTF-8 byte lengths;
    /// keys, containers, and caller-owned returned copies are additional memory.
    pub fn with_object_cache(mut self, bytes: usize) -> Self {
        self.cache = (bytes > 0).then(|| super::object_cache::ObjectCache::new(bytes));
        self
    }
    /// Select bounded immutable chunks for new writes. Upgraded readers accept
    /// both layouts. Staging still reads and compares every selected object.
    pub fn with_chunks(mut self, options: JournalChunkOptions) -> Result<Self, LinkError> {
        self.chunks = Some(options.validate()?);
        Ok(self)
    }
    fn read_object(
        &mut self,
        partition: &str,
        reference: &chunks::Reference,
    ) -> Result<Vec<String>, LinkError> {
        if let Some(entries) = self
            .cache
            .as_mut()
            .and_then(|cache| cache.get(partition, &reference.key, reference.generation))
        {
            chunks::validate(&entries, reference.records)?;
            return Ok(entries);
        }
        let store = self
            .partitions
            .get_mut(partition)
            .expect("validated partition");
        let object = read_one(store, &reference.key)?;
        if object.generation != reference.generation || address(&object.entries) != reference.key {
            return Err(error("missing or modified immutable object"));
        }
        chunks::validate(&object.entries, reference.records)?;
        if let Some(cache) = &mut self.cache {
            cache.remember(
                partition,
                &reference.key,
                reference.generation,
                &object.entries,
            );
        }
        Ok(object.entries)
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
    let value = values.remove(0);
    if value.generation == 0 && !value.entries.is_empty() {
        return Err(error("invalid absent immutable object"));
    }
    Ok(value)
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
        // Validate every selected pointer before the first data-store access.
        let mut selected = Vec::new();
        for value in pointers {
            if value.generation == 0 && value.entries.is_empty() {
                selected.push((0, None));
                continue;
            }
            if value.generation == 0 {
                return Err(error("invalid catalog generation"));
            }
            let manifest = chunks::manifest(&value.entries)?;
            if !self.partitions.contains_key(&manifest.partition) {
                return Err(error("invalid catalog pointer partition"));
            }
            selected.push((value.generation, Some(manifest)));
        }
        selected
            .into_iter()
            .map(|(generation, manifest)| {
                let mut entries = Vec::new();
                if let Some(manifest) = manifest {
                    for reference in manifest.references {
                        entries.extend(self.read_object(&manifest.partition, &reference)?);
                    }
                }
                Ok(JournalVersion {
                    generation,
                    entries,
                })
            })
            .collect()
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
                    .map(|data| {
                        let partition = (self.route)(key);
                        if !self.partitions.contains_key(&partition) {
                            return Err(error("route must select a configured partition"));
                        }
                        let chunks = match self.chunks {
                            Some(options) => options.split(data)?,
                            None => vec![data.as_slice()],
                        };
                        Ok((partition, chunks))
                    })
                    .transpose()
            })
            .collect::<Result<_, _>>()?;
        let mut pointers = Vec::new();
        for (update, route) in updates.iter().zip(routes) {
            let Some(data) = update else {
                pointers.push(None);
                continue;
            };
            let (partition, objects) = route.expect("validated route for every update");
            let mut manifest = if self.chunks.is_some() {
                vec![
                    "chunks1".into(),
                    partition.clone(),
                    data.len().to_string(),
                    objects.len().to_string(),
                ]
            } else {
                vec!["blob1".into(), partition.clone()]
            };
            let store = self
                .partitions
                .get_mut(&partition)
                .expect("validated partition");
            for data in objects {
                let key = address(data);
                let mut object = read_one(store, &key)?;
                if object.generation == 0 {
                    // A lost response leaves an orphan. Never replay a mutation.
                    store.compare_exchange(
                        std::slice::from_ref(&key),
                        &[object],
                        &[Some(data.to_vec())],
                    )?;
                    object = read_one(store, &key)?;
                }
                if object.generation == 0 || object.entries != data {
                    return Err(error(
                        "immutable object collision or inconsistent data store",
                    ));
                }
                manifest.extend([key, object.generation.to_string()]);
                if self.chunks.is_some() {
                    manifest.push(data.len().to_string());
                }
            }
            pointers.push(Some(manifest));
        }
        self.catalog.compare_exchange(keys, expected, &pointers)
    }
}
