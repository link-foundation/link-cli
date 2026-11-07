//! Sealed sorted journal records on immutable native doublets pages.
//! Full bounded startup validation; point reads retain no payload map.
use super::{format::*, read_committed, Cursor, PageDevice, PagedLog};
use crate::{LinkError, TransitionLogStore};
use serde_json::Value;

const MARKER: &str = "[\"native-sst1\"]";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortedJournalEntry {
    pub key: String,
    /// Positive, nonreused identity, retained even for a tombstone.
    pub generation: u64,
    pub entries: Option<Vec<String>>,
}
#[derive(Clone, Copy, Debug)]
pub struct SortedTableOptions {
    pub max_keys: usize,
    /// Total encoded UTF-8 log record bytes, including the format marker.
    pub max_bytes: usize,
}
impl Default for SortedTableOptions {
    fn default() -> Self {
        Self {
            max_keys: 4096,
            max_bytes: 8_388_608,
        }
    }
}
impl SortedTableOptions {
    fn pages(self) -> Result<u64, LinkError> {
        if !(1..=65_536).contains(&self.max_keys) || !(1..=67_108_864).contains(&self.max_bytes) {
            return Err(invalid("sorted table admission bounds"));
        }
        Ok((self.max_bytes.div_ceil(7) as u64 + 2 * (self.max_keys as u64 + 1)).div_ceil(CELLS))
    }
}
struct Index {
    key: String,
    generation: u64,
    tombstone: bool,
    root: u64,
    previous: u64,
}
/// Caller owns an exclusive immutable device. Open validates the whole file;
/// get checks selected physical pages against its retained identity/hashes.
/// Read failure never writes, retries a mutation or changes ownership.
pub struct SortedJournalTable<D> {
    device: D,
    state: Metadata,
    index: Vec<Index>,
    hashes: Vec<u64>,
    stored_bytes: usize,
}
impl<D: PageDevice> SortedJournalTable<D> {
    pub fn create(
        mut device: D,
        entries: &[SortedJournalEntry],
        options: SortedTableOptions,
    ) -> Result<Self, LinkError> {
        let max_pages = options.pages()?;
        if entries.len() > options.max_keys {
            return Err(invalid("sorted table key limit"));
        }
        let mut records = vec![MARKER.to_owned()];
        let mut bytes = MARKER.len();
        let mut previous = "";
        for entry in entries {
            validate_key(&entry.key)?;
            if entry.key.as_str() <= previous || entry.generation == 0 {
                return Err(invalid("sorted table ordering or generation"));
            }
            let record =
                serde_json::to_string(&(&entry.key, entry.generation.to_string(), &entry.entries))
                    .map_err(|_| invalid("sorted table encoding"))?;
            bytes += record.len();
            if bytes > options.max_bytes {
                return Err(invalid("sorted table byte limit"));
            }
            records.push(record);
            previous = &entry.key;
        }
        if bytes > options.max_bytes {
            return Err(invalid("sorted table byte limit"));
        }
        let raw = [device.read_metadata(0)?, device.read_metadata(1)?];
        if raw.iter().flatten().any(|value| !value.is_empty()) || device.data_length()? != 0 {
            return Err(invalid("sorted table creation requires an empty device"));
        }
        let mut log = PagedLog::with_max_pages(device, max_pages)?;
        for record in records {
            log.append_log_entry(&record)?;
        }
        log.flush_log()?;
        Self::open(log.into_device(), options)
    }
    pub fn open(mut device: D, options: SortedTableOptions) -> Result<Self, LinkError> {
        let max_pages = options.pages()?;
        let flags = device.metadata_flags();
        if flags > 1 {
            return Err(invalid("unsupported sorted table device mode"));
        }
        let raw = [device.read_metadata(0)?, device.read_metadata(1)?];
        let states: Vec<_> = raw
            .iter()
            .map(|value| Metadata::decode(value.as_deref().unwrap_or_default(), flags))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        if states.len() == 2
            && (states[0].identity != states[1].identity
                || (states[0].generation == states[1].generation && raw[0] != raw[1]))
        {
            return Err(invalid("conflicting sorted table decisions"));
        }
        let state = states
            .into_iter()
            .max_by_key(|value| value.generation)
            .ok_or_else(|| invalid("no sealed sorted table decision"))?;
        if state.generation != 1
            || state.records > options.max_keys as u64 + 1
            || state.pages > max_pages
            || device.data_length()? != state.pages * PAGE_SIZE as u64
        {
            return Err(invalid("sorted table seal, length or admission bounds"));
        }
        let records = read_committed(&mut device, &state, true)?;
        if records.first().map(String::as_str) != Some(MARKER) {
            return Err(invalid("unsupported sorted table marker"));
        }
        let mut index = Vec::with_capacity(records.len() - 1);
        let mut bytes = 0;
        let mut root = 0;
        let mut previous = "";
        let mut previous_root = 0;
        for (position, record) in records.iter().enumerate() {
            bytes += record.len();
            if bytes > options.max_bytes {
                return Err(invalid("sorted table byte limit"));
            }
            root += record.len().div_ceil(7) as u64 + 1;
            if position != 0 {
                let entry = decode(record)?;
                if entry.key.as_str() <= previous {
                    return Err(invalid("sorted table key ordering"));
                }
                index.push(Index {
                    key: entry.key,
                    generation: entry.generation,
                    tombstone: entry.entries.is_none(),
                    root,
                    previous: previous_root,
                });
                previous = &index.last().expect("inserted index").key;
            }
            previous_root = root;
        }
        // One flush fills every intermediate page, making these physical
        // record addresses canonical. Bind the index to the validated page chain.
        if root != state.root || root != state.rows {
            return Err(invalid("sorted table packed addresses"));
        }
        let mut cursor = Cursor {
            device: &mut device,
            state: &state,
            index: state.pages as i64 - 1,
            generation: 1,
            expected: state.last_hash,
            page: [0; PAGE_SIZE],
            count: 0,
            consumed: 0,
        };
        let mut hashes = vec![0; state.pages as usize];
        while cursor.index >= 0 {
            cursor.load()?;
            hashes[cursor.index as usize] = u64_at(&cursor.page, 56);
            cursor.index -= 1;
        }
        device.complete_recovery()?;
        Ok(Self {
            device,
            state,
            index,
            hashes,
            stored_bytes: bytes,
        })
    }
    pub fn len(&self) -> usize {
        self.index.len()
    }
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
    pub fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
    pub fn identity(&self) -> [u8; 16] {
        self.state.identity
    }
    pub fn digest(&self) -> u64 {
        self.state.last_hash
    }
    pub fn supports_durable_flush(&self) -> bool {
        self.device.supports_durable_flush()
    }
    pub fn into_device(self) -> D {
        self.device
    }
    pub fn get(&mut self, key: &str) -> Result<Option<SortedJournalEntry>, LinkError> {
        validate_key(key)?;
        match self
            .index
            .binary_search_by(|entry| entry.key.as_str().cmp(key))
        {
            Ok(position) => self.at(position).map(Some),
            Err(_) => Ok(None),
        }
    }
    pub fn entries(&mut self) -> Result<Vec<SortedJournalEntry>, LinkError> {
        (0..self.index.len())
            .map(|position| self.at(position))
            .collect()
    }
    fn at(&mut self, position: usize) -> Result<SortedJournalEntry, LinkError> {
        let entry = &self.index[position];
        let page = (entry.root - 1) / CELLS;
        let mut cursor = Cursor {
            device: &mut self.device,
            state: &self.state,
            index: page as i64,
            generation: 1,
            expected: self.hashes[page as usize],
            page: [0; PAGE_SIZE],
            count: 0,
            consumed: 0,
        };
        cursor.load()?;
        cursor.count = ((entry.root - 1) % CELLS + 1) as usize;
        let (source, previous) = cursor.take(entry.root)?;
        let mut string = portable(source)?;
        let mut bytes = Vec::new();
        while string != 0 {
            let (external, next) = cursor.take(string)?;
            let length = ((external >> 56) & 7) as usize;
            let payload = external & PAYLOAD;
            if external & EXTERNAL == 0
                || external & 0x7800000000000000 != 0
                || length == 0
                || payload >> (length * 8) != 0
                || (next != 0 && length != 7)
            {
                return Err(invalid("sorted table string doublet"));
            }
            bytes.extend_from_slice(&payload.to_le_bytes()[..length]);
            string = next;
        }
        if previous != entry.previous || cursor.current() != previous {
            return Err(invalid("sorted table record predecessor"));
        }
        let record = String::from_utf8(bytes).map_err(|_| invalid("sorted table UTF-8"))?;
        let value = decode(&record)?;
        if value.key != entry.key
            || value.generation != entry.generation
            || value.entries.is_none() != entry.tombstone
        {
            return Err(invalid("sorted table index identity"));
        }
        Ok(value)
    }
}
fn validate_key(key: &str) -> Result<(), LinkError> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    {
        return Err(invalid("sorted table key"));
    }
    Ok(())
}
fn decode(record: &str) -> Result<SortedJournalEntry, LinkError> {
    let value: Value =
        serde_json::from_str(record).map_err(|_| invalid("sorted table record JSON"))?;
    let row = value
        .as_array()
        .filter(|row| row.len() == 3)
        .ok_or_else(|| invalid("sorted table record shape"))?;
    let key = row[0]
        .as_str()
        .ok_or_else(|| invalid("sorted table key type"))?;
    validate_key(key)?;
    let text = row[1]
        .as_str()
        .ok_or_else(|| invalid("sorted table generation type"))?;
    let generation = text
        .parse::<u64>()
        .map_err(|_| invalid("sorted table generation"))?;
    if generation == 0 || generation.to_string() != text {
        return Err(invalid("sorted table canonical generation"));
    }
    let entries = if row[2].is_null() {
        None
    } else {
        Some(
            row[2]
                .as_array()
                .ok_or_else(|| invalid("sorted table records"))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| invalid("sorted table record type"))
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
    };
    Ok(SortedJournalEntry {
        key: key.into(),
        generation,
        entries,
    })
}
