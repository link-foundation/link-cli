//! Incremental logical redo; not a physical page WAL or a standalone snapshot.
use super::{error, journal, Row, Snapshot};
use crate::{GenericLink, LinkError};
use doublets::data::LinkReference;
use std::{collections::BTreeSet, sync::Arc};

pub(super) fn encode<T: LinkReference>(
    snapshot: &Snapshot<T>,
    next: u128,
    previous: &Snapshot<T>,
    writes: &[T],
) -> String {
    let payload = writes
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|id| snapshot.rows.get(id) != previous.rows.get(id))
        .map(|id| match snapshot.rows.get(&id) {
            Some(row) => format!(
                "+{id},{},{},{}",
                row.link.source,
                row.link.target,
                row.name
                    .as_ref()
                    .map_or_else(|| "-".into(), |name| journal::hex(name.as_bytes()))
            ),
            None => format!("-{id}"),
        })
        .collect::<Vec<_>>()
        .join(";");
    let text = format!(
        "engine2|{}|{}|{next}|{payload}",
        snapshot.revision, previous.revision
    );
    format!("{text}|{:016x}", journal::checksum(&text))
}

fn decimal(text: &str) -> Result<&str, LinkError> {
    if text.is_empty()
        || !text.bytes().all(|byte| byte.is_ascii_digit())
        || (text.len() > 1 && text.starts_with('0'))
    {
        return Err(error("noncanonical change decimal"));
    }
    Ok(text)
}
fn address<T: LinkReference>(text: &str) -> Result<T, LinkError> {
    journal::address(decimal(text)?)
}
fn lower_hex(text: &str) -> bool {
    text.bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn decode<T: LinkReference>(
    entry: &str,
    previous: &Snapshot<T>,
) -> Result<(Snapshot<T>, u128), LinkError> {
    let text = journal::checked_text(entry)?;
    if !lower_hex(entry.rsplit_once('|').expect("validated checksum").1) {
        return Err(error("noncanonical change checksum"));
    }
    let parts: Vec<_> = text.split('|').collect();
    if parts.len() != 5 || parts[0] != "engine2" {
        return Err(error("unsupported changes format"));
    }
    let revision = decimal(parts[1])?
        .parse::<u64>()
        .map_err(|_| error("invalid change revision"))?;
    let base = decimal(parts[2])?
        .parse::<u64>()
        .map_err(|_| error("invalid change base"))?;
    let next = decimal(parts[3])?
        .parse::<u128>()
        .map_err(|_| error("invalid change allocator"))?;
    if base != previous.revision || next == 0 {
        return Err(error("change base or allocator mismatch"));
    }
    let mut rows = previous.rows.as_ref().clone();
    let mut last = None;
    if !parts[4].is_empty() {
        for operation in parts[4].split(';') {
            let (id, row) = if let Some(fields) = operation.strip_prefix('+') {
                let fields: Vec<_> = fields.split(',').collect();
                if fields.len() != 4 {
                    return Err(error("invalid change row"));
                }
                let id = address::<T>(fields[0])?;
                let name = if fields[3] == "-" {
                    None
                } else {
                    if !lower_hex(fields[3]) {
                        return Err(error("noncanonical change name"));
                    }
                    Some(journal::unhex(fields[3])?)
                };
                (
                    id,
                    Some(Row {
                        link: GenericLink::new(id, address(fields[1])?, address(fields[2])?),
                        name,
                    }),
                )
            } else if let Some(id) = operation.strip_prefix('-') {
                (address::<T>(id)?, None)
            } else {
                return Err(error("unknown change operation"));
            };
            let value: u128 = id
                .try_into()
                .map_err(|_| error("address conversion failed"))?;
            if value == 0 || value >= next || last.is_some_and(|prior| prior >= id) {
                return Err(error(
                    "change addresses must be ordered, unique and inside allocator",
                ));
            }
            last = Some(id);
            if row.as_ref() == rows.get(&id) {
                return Err(error("change has no row effect"));
            }
            match row {
                Some(row) => rows.insert(id, row),
                None => rows.remove(&id),
            }
        }
    }
    Ok((
        Snapshot {
            revision,
            rows: Arc::new(rows),
        },
        next,
    ))
}
