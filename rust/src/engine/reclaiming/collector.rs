use super::*;
impl<C: JournalMaintenanceStore, D: JournalMaintenanceStore>
    ReclaimingPartitionedJournalStore<C, D>
{
    /// Advance one epoch and retire only unprotected objects below that fence.
    /// Unknown mutations return without replay; prior acknowledged cuts remain.
    pub fn collect_garbage(&mut self) -> Result<Option<ReclamationResult>, LinkError> {
        let before = codec::read(&mut self.catalog, &[codec::EPOCH_KEY.into()])?.remove(0);
        let fence = codec::epoch(&before)?
            .checked_add(1)
            .ok_or_else(|| error("reclamation epoch exhausted"))?;
        if !self.catalog.compare_exchange(
            &[codec::EPOCH_KEY.into()],
            &[before],
            &[Some(vec!["gc1".into(), fence.to_string()])],
        )? {
            return Ok(None);
        }
        let entries = codec::scan(&mut self.catalog, self.options, false)?;
        let (epoch, protected) = codec::catalog(&entries, &self.partitions)?;
        if epoch < fence {
            return Err(error("catalog snapshot predates reclamation fence"));
        }
        // Generic providers may alias a namespace; retain live keys across names.
        let protected_keys: std::collections::BTreeSet<_> =
            protected.keys().map(|(_, key)| key.as_str()).collect();
        let mut snapshots = Vec::new();
        for (partition, provider) in &mut self.partitions {
            let entries = codec::scan(provider, self.options, true)?;
            let mut observed = BTreeMap::new();
            for item in &entries {
                codec::key_epoch(&item.key)?;
                observed.insert(item.key.as_str(), item.version.generation);
            }
            for ((name, key), generation) in &protected {
                if name == partition && observed.get(key.as_str()) != Some(generation) {
                    return Err(error("missing or changed protected object"));
                }
            }
            snapshots.push((partition.clone(), entries));
        }
        // Every complete catalog/data snapshot validates before the first delete.
        let mut result = ReclamationResult {
            epoch: fence,
            examined: 0,
            deleted: 0,
        };
        for (partition, entries) in snapshots {
            let provider = self
                .partitions
                .get_mut(&partition)
                .expect("captured partition");
            for item in entries {
                result.examined = result
                    .examined
                    .checked_add(1)
                    .ok_or_else(|| error("reclamation count overflow"))?;
                if codec::key_epoch(&item.key)? >= fence
                    || protected_keys.contains(item.key.as_str())
                {
                    continue;
                }
                if provider.compare_delete(&item.key, item.version.generation)? {
                    result.deleted += 1;
                }
            }
        }
        Ok(Some(result))
    }
}
