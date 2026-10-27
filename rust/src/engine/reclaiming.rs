//! Explicit epoch-fenced reclamation for separately owned immutable namespaces.
use super::{
    cluster::{validate_keys, AtomicJournalStore, JournalVersion},
    error,
    maintenance::{JournalMaintenanceStore, JournalNamespaceEntry, JournalScanOptions},
    object_cache::ObjectCache,
    partitioned::JournalChunkOptions,
};
use crate::LinkError;
use std::collections::BTreeMap;
mod codec;
mod collector;
mod objects;

#[derive(Clone, Copy, Debug)]
pub struct ReclamationOptions {
    pub max_keys: usize,
    pub max_bytes: usize,
    pub page_keys: usize,
    /// Zero retains no cache. Payload accounting matches PartitionedJournalStore.
    pub cache_bytes: usize,
}
impl Default for ReclamationOptions {
    fn default() -> Self {
        Self {
            max_keys: 4096,
            max_bytes: 8_388_608,
            page_keys: 128,
            cache_bytes: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReclamationResult {
    pub epoch: u64,
    pub examined: usize,
    pub deleted: usize,
}
/// Owns the supplied provider values. Use dedicated new namespaces; blob1
/// catalogs are incompatible. No collection or background task runs automatically.
pub struct ReclaimingPartitionedJournalStore<C, D> {
    catalog: C,
    partitions: BTreeMap<String, D>,
    route: Box<dyn Fn(&str) -> String + Send + Sync>,
    options: ReclamationOptions,
    cache: Option<ObjectCache>,
    chunks: Option<JournalChunkOptions>,
}
struct Snapshot {
    values: Vec<JournalVersion>,
    pointers: Vec<Option<codec::Pointer>>,
    root: JournalVersion,
    epoch: u64,
}
enum Group {
    Complete(Vec<JournalVersion>),
    Missing(usize),
}
impl<C: JournalMaintenanceStore, D: JournalMaintenanceStore>
    ReclaimingPartitionedJournalStore<C, D>
{
    pub fn open(
        catalog: C,
        partitions: BTreeMap<String, D>,
        route: impl Fn(&str) -> String + Send + Sync + 'static,
        options: ReclamationOptions,
    ) -> Result<Self, LinkError> {
        Self::open_configured(catalog, partitions, route, options, None)
    }
    /// Select bounded chunks for new writes. Existing blob2 and chunks2 roots
    /// remain readable. Every staged chunk uses the captured catalog epoch.
    pub fn open_chunked(
        catalog: C,
        partitions: BTreeMap<String, D>,
        route: impl Fn(&str) -> String + Send + Sync + 'static,
        options: ReclamationOptions,
        chunks: JournalChunkOptions,
    ) -> Result<Self, LinkError> {
        Self::open_configured(
            catalog,
            partitions,
            route,
            options,
            Some(chunks.validate()?),
        )
    }
    fn open_configured(
        mut catalog: C,
        mut partitions: BTreeMap<String, D>,
        route: impl Fn(&str) -> String + Send + Sync + 'static,
        options: ReclamationOptions,
        chunks: Option<JournalChunkOptions>,
    ) -> Result<Self, LinkError> {
        if options.max_keys == 0
            || options.max_bytes == 0
            || !(1..=256).contains(&options.page_keys)
        {
            return Err(error("invalid reclamation limits"));
        }
        if partitions.is_empty() {
            return Err(error("provide named maintenance-capable data stores"));
        }
        for key in partitions.keys() {
            validate_keys(std::slice::from_ref(key))?;
        }
        let mut entries = codec::scan(&mut catalog, options, false)?;
        if entries.is_empty() {
            for provider in partitions.values_mut() {
                if !codec::scan(provider, options, true)?.is_empty() {
                    return Err(error("initial reclamation namespaces must be empty"));
                }
            }
            // Exactly one initialization attempt; an unknown result propagates.
            catalog.compare_exchange(
                &[codec::EPOCH_KEY.into()],
                &[JournalVersion::default()],
                &[Some(vec!["gc1".into(), "0".into()])],
            )?;
            entries = codec::scan(&mut catalog, options, false)?;
        }
        codec::catalog(&entries, &partitions)?;
        Ok(Self {
            catalog,
            partitions,
            route: Box::new(route),
            options,
            cache: (options.cache_bytes > 0).then(|| ObjectCache::new(options.cache_bytes)),
            chunks,
        })
    }
    fn snapshot(&mut self, keys: &[String]) -> Result<Snapshot, LinkError> {
        let mut selected = keys.to_vec();
        selected.push(codec::EPOCH_KEY.into());
        let mut values = codec::read(&mut self.catalog, &selected)?;
        let root = values.pop().expect("validated snapshot length");
        let epoch = codec::epoch(&root)?;
        let pointers = values
            .iter()
            .map(|value| codec::pointer(value, epoch, &self.partitions))
            .collect::<Result<_, _>>()?;
        Ok(Snapshot {
            values,
            pointers,
            root,
            epoch,
        })
    }
    fn group(&mut self, snapshot: &Snapshot) -> Result<Group, LinkError> {
        let mut result = Vec::new();
        for (index, (item, pointer)) in snapshot.values.iter().zip(&snapshot.pointers).enumerate() {
            let Some(pointer) = pointer else {
                result.push(JournalVersion::default());
                continue;
            };
            let mut entries = Vec::new();
            for reference in &pointer.references {
                let Some(records) = self.read_object(&pointer.partition, reference)? else {
                    return Ok(Group::Missing(index));
                };
                entries.extend(records);
            }
            result.push(JournalVersion {
                generation: item.generation,
                entries,
            });
        }
        Ok(Group::Complete(result))
    }
}
impl<C: JournalMaintenanceStore, D: JournalMaintenanceStore> AtomicJournalStore
    for ReclaimingPartitionedJournalStore<C, D>
{
    fn supports_durable_flush(&self) -> bool {
        self.catalog.supports_durable_flush()
            && self
                .partitions
                .values()
                .all(AtomicJournalStore::supports_durable_flush)
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        codec::keys(keys)?;
        let first = self.snapshot(keys)?;
        let missing = match self.group(&first)? {
            Group::Complete(result) => return Ok(result),
            Group::Missing(index) => index,
        };
        let fresh = self.snapshot(keys)?;
        if first.values[missing].generation == fresh.values[missing].generation
            || first.values[missing].entries == fresh.values[missing].entries
        {
            return Err(error("missing or modified referenced reclamation object"));
        }
        // Discard the entire first group, even already owned/cache-hit entries.
        match self.group(&fresh)? {
            Group::Complete(result) => Ok(result),
            Group::Missing(_) => Err(error("reclamation object retired during bounded retry")),
        }
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        codec::keys(keys)?;
        if expected.len() != keys.len() || updates.len() != keys.len() {
            return Err(error("expected generations and updates must match shards"));
        }
        let guards: Vec<_> = expected
            .iter()
            .map(|item| JournalVersion {
                generation: item.generation,
                entries: vec![],
            })
            .collect();
        let staged = keys
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
                            Some(options) => options
                                .split(data)?
                                .into_iter()
                                .map(<[String]>::to_vec)
                                .collect(),
                            None => vec![data.clone()],
                        };
                        Ok((partition, data.len(), chunks))
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let current = self.snapshot(keys)?;
        if current
            .values
            .iter()
            .zip(&guards)
            .any(|(a, b)| a.generation != b.generation)
        {
            return Ok(false);
        }
        let mut pointers = Vec::new();
        for item in staged {
            let Some((partition, total, chunks)) = item else {
                pointers.push(None);
                continue;
            };
            let mut pointer = if self.chunks.is_some() {
                vec![
                    "chunks2".into(),
                    partition.clone(),
                    total.to_string(),
                    chunks.len().to_string(),
                ]
            } else {
                vec!["blob2".into(), partition.clone()]
            };
            for records in chunks {
                let (key, generation) = self.stage_object(&partition, current.epoch, &records)?;
                pointer.extend([key, generation.to_string()]);
                if self.chunks.is_some() {
                    pointer.push(records.len().to_string());
                }
            }
            pointers.push(Some(pointer));
        }
        let mut selected = keys.to_vec();
        selected.push(codec::EPOCH_KEY.into());
        let mut guards = guards;
        guards.push(current.root);
        pointers.push(None);
        self.catalog.compare_exchange(&selected, &guards, &pointers)
    }
}
