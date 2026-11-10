//! Mandatory v3 undo metadata shares the transition namespace so old readers fail.
use std::collections::BTreeMap;

use doublets::data::LinkReference;

use super::record::{decode_name, encode_name};
use crate::{GenericLink, LinkError};

/// Undo source for the ordinary, immediately mutating storage adapter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowUndoPolicy {
    /// Record post-effect observations. A crash before observation is not atomic.
    #[default]
    Observed,
    /// Flush a complete logical row/name image before an owner's first write.
    /// Costs O(N log N) capture and O(N + name bytes) memory/journal per owner.
    /// Each write also scans rows/names and reconciles pending record effects.
    WriteAheadImage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UndoImage<T> {
    pub(super) owner: u128,
    pub(super) base: i64,
    pub(super) rows: Vec<GenericLink<T>>,
    pub(super) names: Option<BTreeMap<T, String>>,
}

impl<T: LinkReference> UndoImage<T> {
    pub(super) fn serialize(&self) -> String {
        let rows = self
            .rows
            .iter()
            .map(|row| format!("{},{},{}", row.index, row.source, row.target))
            .collect::<Vec<_>>()
            .join(";");
        let names = self
            .names
            .as_ref()
            .map(|names| {
                names
                    .iter()
                    .map(|(index, name)| format!("{index},{}", encode_name(Some(name))))
                    .collect::<Vec<_>>()
                    .join(";")
            })
            .unwrap_or_else(|| "-".into());
        format!("v3|undo|{:032x}|{}|{rows}|{names}", self.owner, self.base)
    }

    pub(super) fn parse(text: &str) -> Result<Self, LinkError> {
        let invalid = || LinkError::InvalidFormat("invalid write-ahead undo image".into());
        let parts = text.split('|').collect::<Vec<_>>();
        if parts.len() != 6 || parts[0] != "v3" || parts[1] != "undo" {
            return Err(invalid());
        }
        let owner = u128::from_str_radix(parts[2], 16).map_err(|_| invalid())?;
        let base = parts[3].parse::<i64>().map_err(|_| invalid())?;
        if owner == 0 || base < 0 {
            return Err(invalid());
        }
        let address = |value: &str| {
            let number = value.parse::<u128>().map_err(|_| invalid())?;
            T::try_from(number).map_err(|_| LinkError::AddressOutOfRange(number))
        };
        let mut rows: Vec<GenericLink<T>> = Vec::new();
        if !parts[4].is_empty() {
            for value in parts[4].split(';') {
                let fields = value.split(',').collect::<Vec<_>>();
                if fields.len() != 3 {
                    return Err(invalid());
                }
                let row = GenericLink::new(
                    address(fields[0])?,
                    address(fields[1])?,
                    address(fields[2])?,
                );
                if row.index == T::from_byte(0)
                    || rows.last().is_some_and(|last| last.index >= row.index)
                {
                    return Err(invalid());
                }
                rows.push(row);
            }
        }
        let names = if parts[5] == "-" {
            None
        } else {
            let mut names = BTreeMap::new();
            if !parts[5].is_empty() {
                for value in parts[5].split(';') {
                    let (index, encoded) = value.split_once(',').ok_or_else(invalid)?;
                    let index = address(index)?;
                    let name = decode_name(encoded).flatten().ok_or_else(invalid)?;
                    if names
                        .last_key_value()
                        .is_some_and(|(last, _)| *last >= index)
                    {
                        return Err(invalid());
                    }
                    names.insert(index, name);
                }
            }
            Some(names)
        };
        let image = Self {
            owner,
            base,
            rows,
            names,
        };
        if image.serialize() != text {
            return Err(invalid());
        }
        Ok(image)
    }
}
