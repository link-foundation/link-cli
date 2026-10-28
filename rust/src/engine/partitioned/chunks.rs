//! Versioned chunk manifests. Write limits are selected independently of caching.
use super::{error, LinkError};

/// Optional immutable chunk layout; whole-journal objects remain the default.
#[derive(Debug, Clone, Copy)]
pub struct JournalChunkOptions {
    pub max_records: usize,
    pub max_bytes: usize,
    pub max_chunks: usize,
}
impl Default for JournalChunkOptions {
    fn default() -> Self {
        Self {
            max_records: 64,
            max_bytes: 65536,
            max_chunks: 1024,
        }
    }
}
impl JournalChunkOptions {
    pub(crate) fn validate(self) -> Result<Self, LinkError> {
        if self.max_records == 0
            || self.max_records > 65536
            || self.max_bytes == 0
            || self.max_bytes > 8388608
            || self.max_chunks == 0
            || self.max_chunks > 4096
        {
            return Err(error("invalid journal chunk limits"));
        }
        Ok(self)
    }
    pub(crate) fn split<'a>(&self, records: &'a [String]) -> Result<Vec<&'a [String]>, LinkError> {
        let mut chunks = Vec::new();
        let (mut start, mut bytes) = (0, 0);
        for (index, record) in records.iter().enumerate() {
            if record.len() > self.max_bytes {
                return Err(error("chunk record exceeds byte limit"));
            }
            if index > start
                && (index - start == self.max_records || record.len() > self.max_bytes - bytes)
            {
                chunks.push(&records[start..index]);
                start = index;
                bytes = 0;
            }
            if chunks.len() >= self.max_chunks {
                return Err(error("journal chunk count limit"));
            }
            bytes += record.len();
        }
        if start < records.len() {
            chunks.push(&records[start..]);
        }
        Ok(chunks)
    }
}
pub(super) struct Reference {
    pub key: String,
    pub generation: u64,
    pub records: Option<usize>,
}
pub(super) struct Manifest {
    pub partition: String,
    pub references: Vec<Reference>,
}
fn decimal(value: &str, max: u64) -> Option<u64> {
    let number: u64 = value.parse().ok()?;
    (number <= max && number.to_string() == value).then_some(number)
}
fn address(value: &str) -> bool {
    value.len() == 22
        && value.starts_with("blob1-")
        && value.as_bytes()[6..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}
pub(super) fn manifest(parts: &[String]) -> Result<Manifest, LinkError> {
    let invalid = || error("invalid catalog pointer or chunk manifest");
    if parts.len() == 4 && parts[0] == "blob1" && address(&parts[2]) {
        let generation = decimal(&parts[3], u64::MAX)
            .filter(|n| *n > 0)
            .ok_or_else(invalid)?;
        return Ok(Manifest {
            partition: parts[1].clone(),
            references: vec![Reference {
                key: parts[2].clone(),
                generation,
                records: None,
            }],
        });
    }
    if parts.len() < 4 || parts[0] != "chunks1" {
        return Err(invalid());
    }
    let total = decimal(&parts[2], u32::MAX.into()).ok_or_else(invalid)?;
    let count = decimal(&parts[3], 4096).ok_or_else(invalid)? as usize;
    if parts.len() != 4 + 3 * count {
        return Err(invalid());
    }
    let mut sum = 0;
    let mut references = Vec::with_capacity(count);
    for triple in parts[4..].as_chunks::<3>().0 {
        if !address(&triple[0]) {
            return Err(invalid());
        }
        let generation = decimal(&triple[1], u64::MAX)
            .filter(|n| *n > 0)
            .ok_or_else(invalid)?;
        let records = decimal(&triple[2], 65536)
            .filter(|n| *n > 0)
            .ok_or_else(invalid)?;
        sum += records;
        references.push(Reference {
            key: triple[0].clone(),
            generation,
            records: Some(records as usize),
        });
    }
    if sum != total {
        return Err(invalid());
    }
    Ok(Manifest {
        partition: parts[1].clone(),
        references,
    })
}
pub(crate) fn validate(records: &[String], count: Option<usize>) -> Result<(), LinkError> {
    if let Some(count) = count {
        if records.len() != count
            || records
                .iter()
                .map(String::len)
                .try_fold(0usize, |sum, n| sum.checked_add(n))
                .is_none_or(|sum| sum > 8388608)
        {
            return Err(error("invalid immutable chunk count or byte bound"));
        }
    }
    Ok(())
}
