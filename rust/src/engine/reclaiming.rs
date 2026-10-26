//! Explicit epoch-fenced reclamation for separately owned immutable namespaces.
use super::{
    cluster::{validate_keys, AtomicJournalStore, JournalVersion},
    error,
    maintenance::{JournalMaintenanceStore, JournalNamespaceEntry, JournalScanOptions},
    object_cache::ObjectCache,
};
use crate::LinkError;
use std::collections::BTreeMap;
mod codec;
mod collector;

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
        mut catalog: C,
        mut partitions: BTreeMap<String, D>,
        route: impl Fn(&str) -> String + Send + Sync + 'static,
        options: ReclamationOptions,
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
            if let Some(entries) = self
                .cache
                .as_mut()
                .and_then(|cache| cache.get(&pointer.partition, &pointer.key, pointer.generation))
            {
                result.push(JournalVersion {
                    generation: item.generation,
                    entries,
                });
                continue;
            }
            let provider = self
                .partitions
                .get_mut(&pointer.partition)
                .expect("validated pointer partition");
            let object = codec::read(provider, std::slice::from_ref(&pointer.key))?.remove(0);
            if object.generation == 0 {
                return Ok(Group::Missing(index));
            }
            if object.generation != pointer.generation
                || codec::address(pointer.epoch, &object.entries) != pointer.key
            {
                return Err(error("modified immutable reclamation object"));
            }
            if let Some(cache) = &mut self.cache {
                cache.remember(
                    &pointer.partition,
                    &pointer.key,
                    pointer.generation,
                    &object.entries,
                );
            }
            result.push(JournalVersion {
                generation: item.generation,
                entries: object.entries,
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
                        Ok((partition, data.clone()))
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
            let Some((partition, data)) = item else {
                pointers.push(None);
                continue;
            };
            let key = codec::address(current.epoch, &data);
            let provider = self
                .partitions
                .get_mut(&partition)
                .expect("validated route");
            let mut object = codec::read(provider, std::slice::from_ref(&key))?.remove(0);
            if object.generation == 0 {
                provider.compare_exchange(
                    std::slice::from_ref(&key),
                    &[object],
                    &[Some(data.clone())],
                )?;
                object = codec::read(provider, std::slice::from_ref(&key))?.remove(0);
            }
            if object.generation == 0 || object.entries != data {
                return Err(error("immutable object collision or staging conflict"));
            }
            pointers.push(Some(vec![
                "blob2".into(),
                partition,
                key,
                object.generation.to_string(),
            ]));
        }
        let mut selected = keys.to_vec();
        selected.push(codec::EPOCH_KEY.into());
        let mut guards = guards;
        guards.push(current.root);
        pointers.push(None);
        self.catalog.compare_exchange(&selected, &guards, &pointers)
    }
}
