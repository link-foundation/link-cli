//! Version-one wire format; independent vectors live in docs/protocol.
use crate::LinkError;

pub const PAGE_SIZE: usize = 4096;
pub const CELLS: u64 = 252;
pub const MAX_PAGES: u64 = ((1_u64 << 53) - 1) / PAGE_SIZE as u64;
pub const EXTERNAL: u64 = 1 << 63;
pub const PAYLOAD: u64 = (1 << 56) - 1;
pub const PAGE_MAGIC: &[u8; 8] = b"LNKDPG01";
const META_MAGIC: &[u8; 8] = b"LNKDMT01";

pub fn invalid(message: &str) -> LinkError {
    LinkError::StorageError(format!("invalid native pages: {message}"))
}
pub fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("fixed wire field"),
    )
}
pub fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed wire field"),
    )
}
pub fn put(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
pub fn portable(value: u64) -> Result<u64, LinkError> {
    if value > (1 << 53) - 1 {
        return Err(invalid("integer exceeds portable range"));
    }
    Ok(value)
}
pub(super) fn hash(bytes: &[u8], initial: u64) -> u64 {
    bytes.iter().fold(initial, |sum, byte| {
        (sum ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
pub fn page_hash(bytes: &[u8; PAGE_SIZE]) -> u64 {
    hash(&bytes[64..], hash(&bytes[..56], 0xcbf29ce484222325))
}

#[derive(Clone)]
pub struct Metadata {
    pub flags: u32,
    pub identity: [u8; 16],
    pub generation: u64,
    pub pages: u64,
    pub rows: u64,
    pub records: u64,
    pub root: u64,
    pub last_hash: u64,
}
impl Metadata {
    pub fn empty(identity: [u8; 16], flags: u32) -> Self {
        Self {
            flags,
            identity,
            generation: 0,
            pages: 0,
            rows: 0,
            records: 0,
            root: 0,
            last_hash: 0,
        }
    }
    pub fn encode(&self) -> [u8; 128] {
        let mut bytes = [0; 128];
        bytes[..8].copy_from_slice(META_MAGIC);
        bytes[8..12].copy_from_slice(&1_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.flags.to_le_bytes());
        bytes[16..32].copy_from_slice(&self.identity);
        for (index, value) in [
            self.generation,
            self.pages,
            self.rows,
            self.records,
            self.root,
            self.last_hash,
        ]
        .into_iter()
        .enumerate()
        {
            put(&mut bytes, 32 + index * 8, value);
        }
        let checksum = hash(&bytes[..120], 0xcbf29ce484222325);
        put(&mut bytes, 120, checksum);
        bytes
    }
    /// Missing/torn decisions have no committed meaning. Checksummed unsupported
    /// versions/flags are errors, not permission to silently select old state.
    pub fn decode(bytes: &[u8], flags: u32) -> Result<Option<Self>, LinkError> {
        if bytes.len() != 128 || hash(&bytes[..120], 0xcbf29ce484222325) != u64_at(bytes, 120) {
            return Ok(None);
        }
        if &bytes[..8] != META_MAGIC || u32_at(bytes, 8) != 1 {
            return Err(invalid("unsupported metadata version"));
        }
        if u32_at(bytes, 12) != flags || bytes[80..120].iter().any(|v| *v != 0) {
            return Err(invalid("unsupported metadata flags"));
        }
        let result = Self {
            flags,
            identity: bytes[16..32].try_into().expect("fixed identity"),
            generation: portable(u64_at(bytes, 32))?,
            pages: portable(u64_at(bytes, 40))?,
            rows: portable(u64_at(bytes, 48))?,
            records: portable(u64_at(bytes, 56))?,
            root: portable(u64_at(bytes, 64))?,
            last_hash: u64_at(bytes, 72),
        };
        let Self {
            generation,
            pages,
            rows,
            records,
            root,
            last_hash,
            ..
        } = result;
        if pages > MAX_PAGES
            || generation > pages
            || rows > pages * CELLS
            || records > rows
            || rows < pages
            || records < generation
        {
            return Err(invalid("metadata counts"));
        }
        if if pages == 0 {
            generation != 0 || rows != 0 || records != 0 || root != 0 || last_hash != 0
        } else {
            generation == 0 || root <= (pages - 1) * CELLS || root > pages * CELLS
        } {
            return Err(invalid("metadata root"));
        }
        Ok(Some(result))
    }
}
