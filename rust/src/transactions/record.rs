//! Complete journal records. The original `Copy` row transition remains v1.
use std::ops::Deref;

use doublets::data::LinkReference;

use super::{GenericDoubletLink, GenericTransition, TransitionKind};
use crate::LinkError;

/// Logical name state for one address, including names of absent rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameChange<T> {
    pub index: T,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// A row transition plus optional name state, or an independent name write.
/// Use this complete value for recovery, archives and version movement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalRecord<T> {
    pub transition: GenericTransition<T>,
    pub name_only: bool,
    pub names: Vec<NameChange<T>>,
}

impl<T> Deref for JournalRecord<T> {
    type Target = GenericTransition<T>;
    fn deref(&self) -> &Self::Target {
        &self.transition
    }
}

impl<T> From<GenericTransition<T>> for JournalRecord<T> {
    fn from(transition: GenericTransition<T>) -> Self {
        Self {
            transition,
            name_only: false,
            names: Vec::new(),
        }
    }
}

impl<T: LinkReference> JournalRecord<T> {
    /// Row-only records retain the exact existing v1 encoding. Complete v2
    /// records append an explicit replay mode and sorted name changes. Names
    /// are UTF-16 hex, `-` is absent, and empty hex is a present empty name.
    pub fn serialize(&self) -> String {
        let row = self.transition.serialize();
        if !self.name_only && self.names.is_empty() {
            return row;
        }
        let names = self
            .names
            .iter()
            .map(|name| {
                format!(
                    "{},{},{}",
                    name.index,
                    encode_name(name.before.as_deref()),
                    encode_name(name.after.as_deref())
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        format!(
            "v2{}|{}|{}",
            &row[2..],
            if self.name_only { "n" } else { "r" },
            names
        )
    }

    pub fn parse(text: &str) -> Result<Self, LinkError> {
        if !text.starts_with("v2|") {
            return GenericTransition::parse(text).map(Into::into);
        }
        let invalid = || LinkError::InvalidFormat("malformed complete transaction record".into());
        let parts: Vec<_> = text.split('|').collect();
        if parts.len() != 9 {
            return Err(invalid());
        }
        let transition = GenericTransition::parse(&format!("v1|{}", parts[1..7].join("|")))?;
        // The complete cross-language codec uses .NET's representable time
        // range. Preserve the original v1 parser's wider legacy contract.
        if !(-62_135_596_800_000..=253_402_300_799_999).contains(&transition.timestamp_ms) {
            return Err(invalid());
        }
        let name_only = match parts[7] {
            "n" => true,
            "r" => false,
            _ => return Err(invalid()),
        };
        let mut names = Vec::new();
        for change in parts[8].split(';') {
            let fields: Vec<_> = change.split(',').collect();
            if fields.len() != 3 {
                return Err(invalid());
            }
            let value = fields[0].parse::<u128>().map_err(|_| invalid())?;
            let index = T::try_from(value).map_err(|_| LinkError::AddressOutOfRange(value))?;
            names.push(NameChange {
                index,
                before: decode_name(fields[1]).ok_or_else(invalid)?,
                after: decode_name(fields[2]).ok_or_else(invalid)?,
            });
        }
        let record = Self {
            transition,
            name_only,
            names,
        };
        record.validate()?;
        // New records use one canonical form; old row codecs remain lenient.
        if record.serialize() != text {
            return Err(invalid());
        }
        Ok(record)
    }

    pub(crate) fn validate(&self) -> Result<(), LinkError> {
        let invalid =
            || LinkError::InvalidFormat("invalid name state in transaction record".into());
        if self
            .names
            .windows(2)
            .any(|pair| pair[0].index >= pair[1].index)
        {
            return Err(invalid());
        }
        if self.name_only {
            if self.names.is_empty()
                || self.kind != TransitionKind::Update
                || self.before != GenericDoubletLink::empty()
                || self.after != GenericDoubletLink::empty()
            {
                return Err(invalid());
            }
        } else if !self.names.is_empty() {
            let index = if self.after.index != T::from_byte(0) {
                self.after.index
            } else {
                self.before.index
            };
            let zero = T::from_byte(0);
            let shape = match self.kind {
                TransitionKind::Create => {
                    self.before == GenericDoubletLink::empty() && self.after.index != zero
                }
                TransitionKind::Update => {
                    self.before.index != zero && self.before.index == self.after.index
                }
                TransitionKind::Delete => {
                    self.before.index != zero && self.after == GenericDoubletLink::empty()
                }
            };
            if !shape || self.names.len() != 1 || self.names[0].index != index || index == zero {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

fn encode_name(name: Option<&str>) -> String {
    name.map(|name| {
        name.encode_utf16()
            .map(|unit| format!("{unit:04X}"))
            .collect()
    })
    .unwrap_or_else(|| "-".into())
}

fn decode_name(text: &str) -> Option<Option<String>> {
    if text == "-" {
        return Some(None);
    }
    if !text.len().is_multiple_of(4) || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let units = text
        .as_bytes()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| u16::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf16(&units).ok().map(Some)
}
