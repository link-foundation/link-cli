//! Portable full-revision journal. The checksum detects accidental corruption;
//! it is not authentication. FileTransitionLog handles newline framing/torn tails.
use super::{changes, error, Row, Rows, Snapshot, StateStructure};
use crate::{GenericLink, LinkError, TransitionLogStore};
use doublets::data::LinkReference;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Default, Debug, Clone)]
pub struct MemoryLog {
    pub entries: Vec<String>,
}
impl TransitionLogStore for MemoryLog {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        self.entries.push(entry.into());
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.entries.clone())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        Ok(())
    }
}
pub(super) fn checksum(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub(super) fn unhex(text: &str) -> Result<String, LinkError> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(error("invalid journal name encoding"));
    }
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&text[i..i + 2], 16)
                .map_err(|_| error("invalid journal name encoding"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf8(bytes).map_err(|_| error("invalid UTF-8 in journal name"))
}
pub(super) fn encode<T: LinkReference>(snapshot: &Snapshot<T>, next: u128) -> String {
    encode_format(snapshot, next, "engine1")
}
pub(super) fn encode_format<T: LinkReference>(
    snapshot: &Snapshot<T>,
    next: u128,
    format: &str,
) -> String {
    let payload = snapshot
        .rows
        .values()
        .map(|row| {
            format!(
                "{},{},{},{}",
                row.link.index,
                row.link.source,
                row.link.target,
                row.name
                    .as_ref()
                    .map_or_else(|| "-".into(), |name| hex(name.as_bytes()))
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    let text = format!("{format}|{}|{next}|{payload}", snapshot.revision);
    format!("{text}|{:016x}", checksum(&text))
}
pub(super) fn address<T: LinkReference>(text: &str) -> Result<T, LinkError> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error("invalid journal address"));
    }
    let value = text
        .parse::<u128>()
        .map_err(|_| error("invalid journal address"))?;
    T::try_from(value).map_err(|_| LinkError::AddressOutOfRange(value))
}

pub(super) fn checked_text(entry: &str) -> Result<&str, LinkError> {
    let (text, hash) = entry
        .rsplit_once('|')
        .ok_or_else(|| error("missing journal checksum"))?;
    if hash.len() != 16
        || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        || u64::from_str_radix(hash, 16).ok() != Some(checksum(text))
    {
        return Err(error("journal checksum mismatch"));
    }
    Ok(text)
}

/// Replays either format against its exact predecessor. Merge-base exports use
/// standalone decode instead, so a dependent record cannot masquerade as a base.
pub(super) fn decode_after<T: LinkReference>(
    entry: &str,
    previous: &Snapshot<T>,
    previous_next: u128,
    structure: StateStructure,
) -> Result<(Snapshot<T>, u128), LinkError> {
    let (mut snapshot, next) = if entry.starts_with("engine2|") {
        changes::decode(entry, previous)?
    } else {
        decode(entry)?
    };
    if snapshot.revision
        != previous
            .revision
            .checked_add(1)
            .ok_or_else(|| error("revision overflow"))?
    {
        return Err(error("journal revisions are not contiguous"));
    }
    if next < previous_next {
        return Err(error("journal allocator moved backwards"));
    }
    snapshot.rows =
        Arc::new(Arc::unwrap_or_clone(snapshot.rows).adapt(structure, Some(&previous.rows)));
    Ok((snapshot, next))
}

pub(super) fn decode<T: LinkReference>(entry: &str) -> Result<(Snapshot<T>, u128), LinkError> {
    decode_format(entry, "engine1")
}
pub(super) fn decode_checkpoint<T: LinkReference>(
    entry: &str,
    structure: StateStructure,
) -> Result<(Snapshot<T>, u128), LinkError> {
    let (mut snapshot, next) = decode_format(entry, "engine3")?;
    if snapshot.revision == 0 && next != 1 {
        return Err(error("invalid initial checkpoint"));
    }
    snapshot.rows = Arc::new(Arc::unwrap_or_clone(snapshot.rows).adapt(structure, None));
    Ok((snapshot, next))
}
fn decode_format<T: LinkReference>(
    entry: &str,
    format: &str,
) -> Result<(Snapshot<T>, u128), LinkError> {
    let text = checked_text(entry)?;
    let parts = text.split('|').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != format {
        return Err(error("unsupported journal format"));
    }
    if parts[1..=2]
        .iter()
        .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(error("invalid journal revision or allocator"));
    }
    let revision = parts[1]
        .parse::<u64>()
        .map_err(|_| error("invalid journal revision"))?;
    let next = parts[2]
        .parse::<u128>()
        .map_err(|_| error("invalid journal allocator"))?;
    if next == 0 {
        return Err(error("invalid journal allocator"));
    }
    let mut rows = BTreeMap::new();
    if !parts[3].is_empty() {
        for text in parts[3].split(';') {
            let fields = text.split(',').collect::<Vec<_>>();
            if fields.len() != 4 {
                return Err(error("invalid journal row"));
            }
            let id = address::<T>(fields[0])?;
            let value: u128 = id
                .try_into()
                .map_err(|_| error("address conversion failed"))?;
            if value == 0 || value >= next {
                return Err(error("journal address outside allocator range"));
            }
            let row = Row {
                link: GenericLink::new(id, address::<T>(fields[1])?, address::<T>(fields[2])?),
                name: if fields[3] == "-" {
                    None
                } else {
                    Some(unhex(fields[3])?)
                },
            };
            if rows.insert(id, row).is_some() {
                return Err(error("duplicate journal address"));
            }
        }
    }
    Ok((
        Snapshot {
            revision,
            rows: Arc::new(Rows::Flat(rows)),
        },
        next,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_wire_vectors() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/protocol/engine-revision-vectors.json"
        ))
        .unwrap();
        for value in vectors["valid"].as_array().unwrap() {
            let text = value.as_str().unwrap();
            let (snapshot, next) = decode::<u32>(text).unwrap();
            assert_eq!(encode(&snapshot, next), text);
        }
        for value in vectors["invalidU32"].as_array().unwrap() {
            let text = value.as_str().unwrap();
            assert!(decode::<u32>(text).is_err(), "accepted {text}");
        }
    }
}
