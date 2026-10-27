//! Optional SQL-free shadow pages for the ordered log, independent of logical
//! revision and replication strategies. See docs/protocol/native-pages.md.
#[cfg(all(feature = "native-pages", any(unix, windows)))]
mod file;
#[cfg(all(feature = "native-pages", any(unix, windows)))]
pub use file::FilePageDevice;
#[cfg(all(feature = "native-pages", any(unix, windows)))]
mod file_checkpoint;
#[cfg(all(feature = "native-pages", any(unix, windows)))]
pub use file_checkpoint::{FileCheckpointOptions, FileCheckpointPagedLog};
mod checkpoint;
mod checkpoint_format;
mod checkpoint_inspect;
mod checkpoint_root;
pub use checkpoint_root::MirroredCheckpointRoot;
mod format;
pub use checkpoint::CheckpointPagedLog;
mod mirrored;
#[cfg(feature = "sorted-tables")]
mod sorted_table;
use crate::{LinkError, TransitionLogStore};
use format::{
    invalid, page_hash, portable, put, u32_at, u64_at, Metadata, CELLS, EXTERNAL, PAGE_MAGIC,
    PAYLOAD,
};
pub use format::{MAX_PAGES, PAGE_SIZE};
pub use mirrored::MirroredPageDevice;
#[cfg(feature = "sorted-tables")]
pub use sorted_table::{SortedJournalEntry, SortedJournalTable, SortedTableOptions};

/// Caller-owned exclusive synchronous storage. Successful sync must persist all
/// preceding writes in that file. Mutation failure requires reopening the log.
pub trait PageDevice {
    /// Immutable physical format; the default preserves single-copy bytes.
    fn metadata_flags(&self) -> u32 {
        0
    }
    fn read_checked_page(
        &mut self,
        index: u64,
        _expected: u64,
    ) -> Result<[u8; PAGE_SIZE], LinkError> {
        self.read_page(index)
    }
    /// Called only after the complete committed prefix validates.
    fn complete_recovery(&mut self) -> Result<(), LinkError> {
        Ok(())
    }
    fn supports_durable_flush(&self) -> bool {
        false
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError>;
    fn data_length(&mut self) -> Result<u64, LinkError>;
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError>;
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError>;
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError>;
    fn read_page(&mut self, index: u64) -> Result<[u8; PAGE_SIZE], LinkError>;
    fn write_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError>;
    fn sync_data(&mut self) -> Result<(), LinkError>;
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError>;
}

/// Privileged repair of an existing page or contiguous missing suffix. The
/// caller must validate a surviving copy and the complete logical prefix first.
pub trait RepairPageDevice: PageDevice {
    fn repair_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError>;
}

/// Immutable committed doublets pages and alternating metadata decisions.
/// Appends stage UTF-8 strings; flush publishes the complete pending group.
/// Read/flush errors fence this owner; reopen and discover an uncertain outcome.
/// Extraction transfers device ownership without flushing pending records.
pub struct PagedLog<D> {
    device: D,
    state: Metadata,
    pending: Vec<String>,
    pending_rows: u64,
    fenced: bool,
    max_pages: u64,
}
impl<D: PageDevice> PagedLog<D> {
    pub fn open(device: D) -> Result<Self, LinkError> {
        Self::with_max_pages(device, MAX_PAGES)
    }
    pub fn with_max_pages(mut device: D, max_pages: u64) -> Result<Self, LinkError> {
        if max_pages == 0 || max_pages > MAX_PAGES {
            return Err(invalid("invalid native page limit"));
        }
        let flags = device.metadata_flags();
        if flags > 1 {
            return Err(invalid("unsupported device mode"));
        }
        let raw = [device.read_metadata(0)?, device.read_metadata(1)?];
        let mut valid = Vec::new();
        for bytes in &raw {
            if let Some(state) = Metadata::decode(bytes.as_deref().unwrap_or_default(), flags)? {
                valid.push(state);
            }
        }
        let state = if valid.is_empty() {
            if raw.iter().flatten().any(|bytes| !bytes.is_empty()) || device.data_length()? != 0 {
                return Err(invalid("no valid metadata decision"));
            }
            let state = Metadata::empty(device.new_database_id()?, flags);
            device.sync_data()?;
            device.write_metadata(0, &state.encode())?;
            device.sync_metadata(0)?;
            state
        } else {
            if valid.len() == 2
                && (valid[0].identity != valid[1].identity
                    || (valid[0].generation == valid[1].generation && raw[0] != raw[1]))
            {
                return Err(invalid("conflicting metadata decisions"));
            }
            valid
                .into_iter()
                .max_by_key(|state| state.generation)
                .expect("nonempty decisions")
        };
        if state.pages > max_pages {
            return Err(invalid("committed pages exceed selected limit"));
        }
        let length = device.data_length()?;
        let committed = state.pages * PAGE_SIZE as u64;
        if length < committed {
            return Err(invalid("truncated committed pages"));
        }
        read_committed(&mut device, &state, false)?;
        device.complete_recovery()?;
        if length > committed {
            device.truncate_data(committed)?;
            device.sync_data()?;
        }
        Ok(Self {
            device,
            state,
            pending: Vec::new(),
            pending_rows: 0,
            fenced: false,
            max_pages,
        })
    }
    pub fn into_device(self) -> D {
        self.device
    }
    fn check(&self) -> Result<(), LinkError> {
        if self.fenced {
            Err(invalid(
                "uncertain native page owner is fenced; reopen the log",
            ))
        } else {
            Ok(())
        }
    }
}
impl<D: PageDevice> TransitionLogStore for PagedLog<D> {
    fn supports_durable_flush(&self) -> bool {
        self.device.supports_durable_flush()
    }
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        self.check()?;
        if entry.contains(['\r', '\n']) {
            return Err(invalid("journal records cannot contain line breaks"));
        }
        let rows = entry.len().div_ceil(7) as u64 + 1;
        if self.state.pages + (self.pending_rows + rows).div_ceil(CELLS) > self.max_pages {
            return Err(invalid("native page address space exhausted"));
        }
        self.pending.push(entry.to_owned());
        self.pending_rows += rows;
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.check()?;
        match read_committed(&mut self.device, &self.state, true).and_then(|records| {
            self.device.complete_recovery()?;
            Ok(records)
        }) {
            Ok(mut records) => {
                records.extend(self.pending.iter().cloned());
                Ok(records)
            }
            Err(error) => {
                self.fenced = true;
                Err(error)
            }
        }
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.check()?;
        if self.pending.is_empty() {
            return Ok(());
        }
        self.fenced = true;
        let mut next = self.state.clone();
        next.generation += 1;
        let mut writer = Writer {
            device: &mut self.device,
            state: &mut next,
            page: [0; PAGE_SIZE],
            count: 0,
        };
        for entry in &self.pending {
            let mut string = 0;
            for chunk in entry.as_bytes().chunks(7).rev() {
                let mut payload = 0_u64;
                for (i, byte) in chunk.iter().enumerate() {
                    payload |= u64::from(*byte) << (i * 8);
                }
                string = writer.cell(EXTERNAL | ((chunk.len() as u64) << 56) | payload, string)?;
            }
            let root = writer.cell(string, writer.state.root)?;
            writer.state.root = root;
            writer.state.records += 1;
        }
        writer.finish()?;
        self.device.sync_data()?;
        let slot = (next.generation % 2) as usize;
        self.device.write_metadata(slot, &next.encode())?;
        self.device.sync_metadata(slot)?;
        self.state = next;
        self.pending.clear();
        self.pending_rows = 0;
        self.fenced = false;
        Ok(())
    }
}
struct Writer<'a, D> {
    device: &'a mut D,
    state: &'a mut Metadata,
    page: [u8; PAGE_SIZE],
    count: usize,
}
impl<D: PageDevice> Writer<'_, D> {
    fn cell(&mut self, source: u64, target: u64) -> Result<u64, LinkError> {
        if self.count == CELLS as usize {
            self.finish()?;
        }
        let id = self.state.pages * CELLS + self.count as u64 + 1;
        put(&mut self.page, 64 + self.count * 16, source);
        put(&mut self.page, 72 + self.count * 16, target);
        self.count += 1;
        self.state.rows += 1;
        Ok(id)
    }
    fn finish(&mut self) -> Result<(), LinkError> {
        self.page[..8].copy_from_slice(PAGE_MAGIC);
        self.page[8..24].copy_from_slice(&self.state.identity);
        put(&mut self.page, 24, self.state.pages);
        put(&mut self.page, 32, self.state.generation);
        self.page[40..44].copy_from_slice(&(self.count as u32).to_le_bytes());
        put(&mut self.page, 48, self.state.last_hash);
        self.state.last_hash = page_hash(&self.page);
        put(&mut self.page, 56, self.state.last_hash);
        self.device.write_page(self.state.pages, &self.page)?;
        self.state.pages += 1;
        self.page.fill(0);
        self.count = 0;
        Ok(())
    }
}
struct Cursor<'a, D> {
    device: &'a mut D,
    state: &'a Metadata,
    index: i64,
    generation: u64,
    expected: u64,
    page: [u8; PAGE_SIZE],
    count: usize,
    consumed: u64,
}
impl<D: PageDevice> Cursor<'_, D> {
    fn load(&mut self) -> Result<(), LinkError> {
        self.page = self
            .device
            .read_checked_page(self.index as u64, self.expected)?;
        let page = &self.page;
        let actual = page_hash(page);
        if &page[..8] != PAGE_MAGIC
            || page[8..24] != self.state.identity
            || u64_at(page, 24) != self.index as u64
            || actual != self.expected
            || actual != u64_at(page, 56)
        {
            return Err(invalid("page checksum or identity"));
        }
        let created = u64_at(page, 32);
        if created == 0
            || created > self.generation
            || created < self.generation - 1
            || (self.index as u64 == self.state.pages - 1 && created != self.generation)
            || (self.index == 0 && created != 1)
        {
            return Err(invalid("page generation"));
        }
        self.generation = created;
        self.count = u32_at(page, 40) as usize;
        self.expected = u64_at(page, 48);
        if self.count == 0
            || self.count > CELLS as usize
            || u32_at(page, 44) != 0
            || page[64 + self.count * 16..].iter().any(|v| *v != 0)
            || (self.index == 0 && self.expected != 0)
        {
            return Err(invalid("page count or padding"));
        }
        Ok(())
    }
    fn current(&self) -> u64 {
        if self.index < 0 {
            0
        } else {
            self.index as u64 * CELLS + self.count as u64
        }
    }
    fn take(&mut self, wanted: u64) -> Result<(u64, u64), LinkError> {
        if wanted == 0 || wanted != self.current() {
            return Err(invalid("noncanonical doublet reference"));
        }
        let offset = 64 + (self.count - 1) * 16;
        let result = (
            u64_at(&self.page, offset),
            portable(u64_at(&self.page, offset + 8))?,
        );
        self.consumed += 1;
        self.count -= 1;
        if self.count == 0 {
            self.index -= 1;
            if self.index >= 0 {
                self.load()?;
            }
        }
        Ok(result)
    }
}
fn read_committed<D: PageDevice>(
    device: &mut D,
    state: &Metadata,
    retain: bool,
) -> Result<Vec<String>, LinkError> {
    let mut result = Vec::new();
    if state.pages == 0 {
        return Ok(result);
    }
    let mut cursor = Cursor {
        device,
        state,
        index: (state.pages - 1) as i64,
        generation: state.generation,
        expected: state.last_hash,
        page: [0; PAGE_SIZE],
        count: 0,
        consumed: 0,
    };
    cursor.load()?;
    let mut root = state.root;
    let mut records = 0;
    while root != 0 {
        let (source, previous) = cursor.take(root)?;
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
                return Err(invalid("string doublet"));
            }
            bytes.extend_from_slice(&payload.to_le_bytes()[..length]);
            string = next;
        }
        let text = String::from_utf8(bytes).map_err(|_| invalid("record UTF-8"))?;
        if text.contains(['\r', '\n']) {
            return Err(invalid("record delimiter"));
        }
        if previous != cursor.current() {
            return Err(invalid("record predecessor"));
        }
        if retain {
            result.push(text);
        }
        root = previous;
        records += 1;
    }
    if cursor.current() != 0 || cursor.consumed != state.rows || records != state.records {
        return Err(invalid("record counts"));
    }
    result.reverse();
    Ok(result)
}
