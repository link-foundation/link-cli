//! Weighted LRU of already verified immutable objects; catalog state is excluded.
use lru::LruCache;

type Key = (String, String, u64);

pub(super) struct ObjectCache {
    capacity: usize,
    used: usize,
    objects: LruCache<Key, (usize, Vec<String>)>,
}
impl ObjectCache {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            used: 0,
            objects: LruCache::unbounded(),
        }
    }
    pub(super) fn get(
        &mut self,
        partition: &str,
        key: &str,
        generation: u64,
    ) -> Option<Vec<String>> {
        self.objects
            .get(&(partition.into(), key.into(), generation))
            .map(|(_, entries)| entries.clone())
    }
    pub(super) fn remember(
        &mut self,
        partition: &str,
        key: &str,
        generation: u64,
        entries: &[String],
    ) {
        // One byte per object and per record bounds even empty strings/objects.
        let mut size = 1;
        for record in entries {
            if record.len() >= self.capacity - size {
                return;
            }
            size += record.len() + 1;
        }
        let key = (partition.into(), key.into(), generation);
        // Called only after a miss; exclusive access prevents duplicate fills.
        while size > self.capacity - self.used {
            let (_, (old_size, _)) = self.objects.pop_lru().expect("retained cache weight");
            self.used -= old_size;
        }
        self.objects.put(key, (size, entries.to_vec()));
        self.used += size;
    }
}
