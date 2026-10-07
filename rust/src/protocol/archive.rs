//! Store archive: a whole links store in binary links notation.
//!
//! An archive is two [`LinksPacket`]s, one after the other:
//!
//! 1. **Links**, without external references: the store link `(a: s t)` is
//!    the doublet `s t` at address `a`. Addresses the store does not use are
//!    holes, so every link keeps its address.
//! 2. **Names**, with external references: one link per named store link,
//!    in address order, holding the external values
//!    `address code-point…`.
//!
//! Both packets use packed widths, so a store of small addresses costs about
//! two bytes per link. [`import_store`] writes every link back at its own
//! address and restores the names, so exporting the imported store gives
//! the same bytes again.

use super::error::{ProtocolError, ProtocolResult};
use crate::lino_database_input::update_link;
use crate::named_type_links::NamedTypeLinks;
use anyhow::{Context, Result};
use links_notation::binary::{DecodeLimits, LinksPacket, Reference};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

/// Writes every link and name of `storage` to `writer`.
pub fn export_store<S>(storage: &mut S, writer: &mut dyn Write) -> Result<()>
where
    S: NamedTypeLinks,
{
    let mut links = storage.all_links();
    links.sort_by_key(|link| link.index);
    let doublets: Vec<(u64, Vec<Reference>)> = links
        .iter()
        .map(|link| {
            (
                u64::from(link.index),
                vec![
                    Reference::Internal(u64::from(link.source)),
                    Reference::Internal(u64::from(link.target)),
                ],
            )
        })
        .collect();
    let mut names = Vec::new();
    for link in &links {
        if let Some(name) = storage.get_name(link.index)? {
            let mut references = vec![Reference::External(u64::from(link.index))];
            references.extend(
                name.chars()
                    .map(|code_point| Reference::External(u64::from(u32::from(code_point)))),
            );
            names.push((names.len() as u64 + 1, references));
        }
    }
    LinksPacket::pack(false, &doublets, true)?.write_to(writer)?;
    LinksPacket::pack(true, &names, true)?.write_to(writer)?;
    Ok(())
}

/// Writes the archive of `storage` to the file at `path`.
pub fn export_store_file<S, P>(storage: &mut S, path: P) -> Result<()>
where
    S: NamedTypeLinks,
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let mut bytes = Vec::new();
    export_store(storage, &mut bytes)?;
    fs::write(path, bytes)
        .with_context(|| format!("Failed to write the store archive: {}", path.display()))
}

/// Reads an archive from `reader` into `storage`: each link is written at
/// its own address, then every name is set.
pub fn import_store<S>(storage: &mut S, reader: &mut dyn Read) -> Result<()>
where
    S: NamedTypeLinks,
{
    let limits = DecodeLimits::unlimited();
    let links = read_packet(reader, &limits, "links")?;
    let names = read_packet(reader, &limits, "names")?;
    let mut trailing = [0u8; 1];
    if reader.read(&mut trailing)? != 0 {
        return Err(ProtocolError::malformed("trailing bytes after the store archive").into());
    }
    let doublets = links
        .links()
        .map(|(address, references)| match references {
            [Reference::Internal(source), Reference::Internal(target)] => Ok((
                store_address(address)?,
                store_address(*source)?,
                store_address(*target)?,
            )),
            _ => Err(ProtocolError::malformed(format!(
                "archive link {address} is not a doublet of link addresses"
            ))),
        })
        .collect::<ProtocolResult<Vec<_>>>()?;
    let named = names
        .links()
        .map(|(_, references)| decode_name(references))
        .collect::<ProtocolResult<Vec<_>>>()?;
    // Every address exists before any link refers to it.
    for &(index, _, _) in &doublets {
        if !storage.exists(index) {
            storage.ensure_created(index);
        }
    }
    for (index, source, target) in doublets {
        update_link(storage, index, source, target)?;
    }
    for (index, name) in named {
        storage.set_name(index, &name)?;
    }
    storage.save()
}

/// Reads the archive in the file at `path` into `storage`.
pub fn import_store_file<S, P>(storage: &mut S, path: P) -> Result<()>
where
    S: NamedTypeLinks,
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let bytes = fs::read(path)
        .with_context(|| format!("Failed to read the store archive: {}", path.display()))?;
    import_store(storage, &mut bytes.as_slice())
}

fn read_packet(
    reader: &mut dyn Read,
    limits: &DecodeLimits,
    part: &str,
) -> ProtocolResult<LinksPacket> {
    LinksPacket::read_from(reader, limits)?.ok_or_else(|| {
        ProtocolError::malformed(format!("the store archive ends before its {part}"))
    })
}

fn store_address(address: u64) -> ProtocolResult<u32> {
    u32::try_from(address).map_err(|_| {
        ProtocolError::malformed(format!("address {address} does not fit a 32-bit store"))
    })
}

fn decode_name(references: &[Reference]) -> ProtocolResult<(u32, String)> {
    let values = references
        .iter()
        .map(|reference| match reference {
            Reference::External(value) => Ok(*value),
            Reference::Internal(_) => Err(ProtocolError::malformed(
                "an archive name holds only external values",
            )),
        })
        .collect::<ProtocolResult<Vec<_>>>()?;
    // A packet link holds at least one reference: arity 0 is malformed.
    let name = values[1..]
        .iter()
        .map(|&code_point| {
            u32::try_from(code_point)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| ProtocolError::malformed(format!("invalid code point {code_point}")))
        })
        .collect::<ProtocolResult<String>>()?;
    Ok((store_address(values[0])?, name))
}
