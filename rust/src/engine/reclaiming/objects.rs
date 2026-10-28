use super::*;
use crate::engine::partitioned::chunks;

impl<C: JournalMaintenanceStore, D: JournalMaintenanceStore>
    ReclaimingPartitionedJournalStore<C, D>
{
    pub(super) fn read_object(
        &mut self,
        partition: &str,
        reference: &codec::Reference,
    ) -> Result<Option<Vec<String>>, LinkError> {
        if let Some(records) = self
            .cache
            .as_mut()
            .and_then(|cache| cache.get(partition, &reference.key, reference.generation))
        {
            chunks::validate(&records, reference.records)?;
            return Ok(Some(records));
        }
        let provider = self
            .partitions
            .get_mut(partition)
            .expect("validated pointer partition");
        let object = codec::read(provider, std::slice::from_ref(&reference.key))?.remove(0);
        if object.generation == 0 {
            return Ok(None);
        }
        if object.generation != reference.generation
            || codec::address(reference.epoch, &object.entries) != reference.key
        {
            return Err(error("modified immutable reclamation object"));
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
        Ok(Some(object.entries))
    }
    pub(super) fn stage_object(
        &mut self,
        partition: &str,
        epoch: u64,
        records: &[String],
    ) -> Result<(String, u64), LinkError> {
        // Never reuse an older epoch's prefix, including a verified cached copy.
        let key = codec::address(epoch, records);
        let provider = self.partitions.get_mut(partition).expect("validated route");
        let mut object = codec::read(provider, std::slice::from_ref(&key))?.remove(0);
        if object.generation == 0 {
            provider.compare_exchange(
                std::slice::from_ref(&key),
                &[object],
                &[Some(records.to_vec())],
            )?;
            object = codec::read(provider, std::slice::from_ref(&key))?.remove(0);
        }
        if object.generation == 0 || object.entries != records {
            return Err(error("immutable object collision or staging conflict"));
        }
        Ok((key, object.generation))
    }
}
