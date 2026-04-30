//! Unicode string and name storage backed by doublet links.
//!
//! This mirrors the C# `UnicodeStringStorage<uint>` implementation used by
//! `NamedLinksDecorator`: strings are stored as `String -> UnicodeSequence`
//! links, Unicode sequences are balanced doublet trees, Unicode symbols are
//! `raw-code-unit -> UnicodeSymbol` links, and names are regular doublet links
//! from an internal or external reference to `Name -> String`.

use anyhow::{bail, Result};

use crate::link_storage::LinkStorage;
use crate::pinned_types::PinnedTypes;

const EXTERNAL_ZERO: u32 = (u32::MAX / 2) + 1;

/// Encodes an external reference the same way `Platform.Data.Hybrid<uint>` does.
pub fn external_reference(value: u32) -> u32 {
    if value == 0 {
        EXTERNAL_ZERO
    } else {
        0u32.wrapping_sub(value)
    }
}

/// Decodes a `Platform.Data.Hybrid<uint>` external reference.
pub fn external_reference_value(value: u32) -> Option<u32> {
    if value == EXTERNAL_ZERO {
        Some(0)
    } else if value >= EXTERNAL_ZERO {
        Some(0u32.wrapping_sub(value))
    } else {
        None
    }
}

fn raw_number_from_address(value: u32) -> u32 {
    external_reference(value)
}

fn address_from_raw_number(value: u32) -> u32 {
    external_reference_value(value).unwrap_or(value)
}

/// Link-backed Unicode string storage with C# pinned type layout.
pub struct UnicodeStringStorage<'a> {
    links: &'a mut LinkStorage,
    type_type: u32,
    unicode_symbol_type: u32,
    unicode_sequence_type: u32,
    string_type: u32,
    empty_string_type: u32,
    name_type: u32,
}

impl<'a> UnicodeStringStorage<'a> {
    pub fn new(links: &'a mut LinkStorage) -> Result<Self> {
        let (
            type_type,
            unicode_symbol_type,
            unicode_sequence_type,
            string_type,
            empty_string_type,
            name_type,
        ) = {
            let mut pinned_types = PinnedTypes::new(links);
            (
                pinned_types.next_type()?,
                pinned_types.next_type()?,
                pinned_types.next_type()?,
                pinned_types.next_type()?,
                pinned_types.next_type()?,
                pinned_types.next_type()?,
            )
        };

        let mut storage = Self {
            links,
            type_type,
            unicode_symbol_type,
            unicode_sequence_type,
            string_type,
            empty_string_type,
            name_type,
        };

        storage.set_name(type_type, "Type")?;
        storage.set_name(unicode_symbol_type, "UnicodeSymbol")?;
        storage.set_name(unicode_sequence_type, "UnicodeSequence")?;
        storage.set_name(string_type, "String")?;
        storage.set_name(empty_string_type, "EmptyString")?;
        storage.set_name(name_type, "Name")?;

        Ok(storage)
    }

    pub fn links_mut(&mut self) -> &mut LinkStorage {
        self.links
    }

    pub fn into_named_links(self) -> NamedLinks<'a> {
        NamedLinks { storage: self }
    }

    pub fn type_type(&self) -> u32 {
        self.type_type
    }

    pub fn unicode_symbol_type(&self) -> u32 {
        self.unicode_symbol_type
    }

    pub fn unicode_sequence_type(&self) -> u32 {
        self.unicode_sequence_type
    }

    pub fn string_type(&self) -> u32 {
        self.string_type
    }

    pub fn empty_string_type(&self) -> u32 {
        self.empty_string_type
    }

    pub fn name_type(&self) -> u32 {
        self.name_type
    }

    pub fn create_string(&mut self, content: &str) -> Result<u32> {
        let string_sequence = self.get_string_sequence(content);
        Ok(self.links.get_or_create(self.string_type, string_sequence))
    }

    pub fn get_string(&self, string_value: u32) -> Result<String> {
        let mut current = string_value;
        for _ in 0..3 {
            let Some(link) = self.links.get(current) else {
                break;
            };
            if link.source == self.string_type {
                return if link.target == self.empty_string_type {
                    Ok(String::new())
                } else {
                    self.unicode_sequence_to_string(link.target)
                };
            }
            current = link.target;
        }
        bail!("The passed link does not contain a string.")
    }

    pub fn unicode_sequence_code_units(&self, string_value: u32) -> Result<Vec<u16>> {
        let sequence = self.unwrap_string_sequence(string_value)?;
        if sequence == self.empty_string_type {
            return Ok(Vec::new());
        }
        let unicode_sequence = self
            .links
            .get(sequence)
            .ok_or_else(|| anyhow::anyhow!("Unicode sequence link {sequence} does not exist."))?;
        if unicode_sequence.target != self.unicode_sequence_type {
            bail!("Link {sequence} is not a Unicode sequence.");
        }
        let symbol_sequence = unicode_sequence.source;
        self.walk_right_sequence(symbol_sequence)
            .into_iter()
            .map(|symbol| self.unicode_symbol_to_code_unit(symbol))
            .collect()
    }

    pub fn get_types(&self) -> Vec<u32> {
        self.links
            .query(None, Some(self.type_type), None)
            .into_iter()
            .map(|link| link.index)
            .collect()
    }

    pub fn is_type(&self, address: u32) -> bool {
        self.links
            .get(address)
            .is_some_and(|link| link.source == self.type_type)
    }

    pub fn get_or_create_type(&mut self, name: &str) -> Result<u32> {
        if let Some(existing) = self.get_by_name(name)? {
            return Ok(existing);
        }

        let type_link = self.links.create(0, 0);
        self.links.update(type_link, self.type_type, type_link)?;
        self.set_name(type_link, name)?;
        Ok(type_link)
    }

    pub fn set_name_for_external_reference(&mut self, link: u32, name: &str) -> Result<u32> {
        self.set_name(external_reference(link), name)
    }

    pub fn get_name_by_external_reference(&self, link: u32) -> Result<Option<String>> {
        self.get_name(external_reference(link))
    }

    pub fn get_external_reference_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        Ok(self.get_by_name(name)?.and_then(external_reference_value))
    }

    pub fn remove_name_by_external_reference(&mut self, external_reference_id: u32) -> Result<()> {
        self.remove_name(external_reference(external_reference_id))
    }

    pub fn set_name(&mut self, link: u32, name: &str) -> Result<u32> {
        let name_sequence = self.create_string(name)?;
        let name_link = self.links.get_or_create(self.name_type, name_sequence);
        Ok(self.links.get_or_create(link, name_link))
    }

    pub fn get_name(&self, link: u32) -> Result<Option<String>> {
        for name_pair in self.links.query(None, Some(link), None) {
            let name_candidate = name_pair.target;
            if self
                .links
                .get(name_candidate)
                .is_some_and(|candidate| candidate.source == self.name_type)
            {
                return self.get_string(name_candidate).map(Some);
            }
        }
        Ok(None)
    }

    pub fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        let name_sequence = self.create_string(name)?;
        let Some(name_link) = self.links.search(self.name_type, name_sequence) else {
            return Ok(None);
        };
        Ok(self
            .links
            .query(None, None, Some(name_link))
            .into_iter()
            .map(|link| link.source)
            .next())
    }

    pub fn remove_name(&mut self, link: u32) -> Result<()> {
        let name_pairs = self
            .links
            .query(None, Some(link), None)
            .into_iter()
            .map(|link| (link.index, link.target))
            .collect::<Vec<_>>();

        for (name_pair, name_candidate) in name_pairs {
            let Some(candidate) = self.links.get(name_candidate).copied() else {
                continue;
            };
            if candidate.source != self.name_type {
                continue;
            }

            if self.links.exists(name_pair) {
                self.links.delete(name_pair)?;
            }

            let still_used = self
                .links
                .query(None, None, Some(name_candidate))
                .into_iter()
                .any(|usage| usage.index != name_pair);
            if !still_used && self.links.exists(name_candidate) {
                self.links.delete(name_candidate)?;
            }
        }

        Ok(())
    }

    fn get_string_sequence(&mut self, content: &str) -> u32 {
        if content.is_empty() {
            self.empty_string_type
        } else {
            self.string_to_unicode_sequence(content)
        }
    }

    fn string_to_unicode_sequence(&mut self, content: &str) -> u32 {
        let symbols = content
            .encode_utf16()
            .map(|code_unit| self.code_unit_to_unicode_symbol(code_unit))
            .collect::<Vec<_>>();
        self.unicode_symbols_to_unicode_sequence(&symbols)
    }

    fn code_unit_to_unicode_symbol(&mut self, code_unit: u16) -> u32 {
        let raw_number = raw_number_from_address(u32::from(code_unit));
        self.links
            .get_or_create(raw_number, self.unicode_symbol_type)
    }

    fn unicode_symbol_to_code_unit(&self, symbol: u32) -> Result<u16> {
        let Some(link) = self.links.get(symbol) else {
            bail!("Unicode symbol link {symbol} does not exist.");
        };
        if link.target != self.unicode_symbol_type {
            bail!("Specified link {symbol} is not a Unicode symbol.");
        }
        let code_unit = address_from_raw_number(link.source);
        Ok(u16::try_from(code_unit)?)
    }

    fn unicode_symbols_to_unicode_sequence(&mut self, symbols: &[u32]) -> u32 {
        if symbols.is_empty() {
            return self.unicode_sequence_type;
        }
        let sequence = self.balanced_variant(symbols);
        self.links
            .get_or_create(sequence, self.unicode_sequence_type)
    }

    fn unicode_sequence_to_string(&self, sequence: u32) -> Result<String> {
        if sequence == self.unicode_sequence_type {
            return Ok(String::new());
        }

        let Some(sequence_link) = self.links.get(sequence) else {
            bail!("Unicode sequence link {sequence} does not exist.");
        };
        if sequence_link.target != self.unicode_sequence_type {
            bail!("Specified link {sequence} is not a Unicode sequence.");
        }

        let code_units = self
            .walk_right_sequence(sequence_link.source)
            .into_iter()
            .map(|symbol| self.unicode_symbol_to_code_unit(symbol))
            .collect::<Result<Vec<_>>>()?;
        Ok(String::from_utf16(&code_units)?)
    }

    fn balanced_variant(&mut self, symbols: &[u32]) -> u32 {
        match symbols.len() {
            0 => 0,
            1 => symbols[0],
            2 => self.links.get_or_create(symbols[0], symbols[1]),
            _ => {
                let mut layer = symbols.to_vec();
                while layer.len() > 2 {
                    let mut next = Vec::with_capacity(layer.len().div_ceil(2));
                    let mut chunks = layer.chunks_exact(2);
                    for pair in &mut chunks {
                        next.push(self.links.get_or_create(pair[0], pair[1]));
                    }
                    if let Some(&remainder) = chunks.remainder().first() {
                        next.push(remainder);
                    }
                    layer = next;
                }
                self.links.get_or_create(layer[0], layer[1])
            }
        }
    }

    fn walk_right_sequence(&self, sequence: u32) -> Vec<u32> {
        let mut output = Vec::new();
        let mut stack = Vec::new();
        let mut element = sequence;

        if self.is_unicode_symbol(element) {
            output.push(element);
            return output;
        }

        loop {
            if self.is_unicode_symbol(element) {
                let Some(popped) = stack.pop() else {
                    break;
                };
                if let Some(link) = self.links.get(popped) {
                    if self.is_unicode_symbol(link.source) {
                        output.push(link.source);
                    }
                    if self.is_unicode_symbol(link.target) {
                        output.push(link.target);
                    }
                    element = link.target;
                } else {
                    break;
                }
            } else {
                let Some(link) = self.links.get(element) else {
                    break;
                };
                stack.push(element);
                element = link.source;
            }
        }

        output
    }

    fn is_unicode_symbol(&self, link: u32) -> bool {
        self.links
            .get(link)
            .is_some_and(|link| link.target == self.unicode_symbol_type)
    }

    fn unwrap_string_sequence(&self, string_value: u32) -> Result<u32> {
        let mut current = string_value;
        for _ in 0..3 {
            let Some(link) = self.links.get(current) else {
                break;
            };
            if link.source == self.string_type {
                return Ok(link.target);
            }
            current = link.target;
        }
        bail!("The passed link does not contain a string.")
    }
}

/// Public facade matching the C# `NamedLinks<uint>` role.
pub struct NamedLinks<'a> {
    storage: UnicodeStringStorage<'a>,
}

impl<'a> NamedLinks<'a> {
    pub fn new(links: &'a mut LinkStorage) -> Result<Self> {
        Ok(UnicodeStringStorage::new(links)?.into_named_links())
    }

    pub fn set_name_for_external_reference(&mut self, link: u32, name: &str) -> Result<u32> {
        self.storage.set_name_for_external_reference(link, name)
    }

    pub fn set_name(&mut self, link: u32, name: &str) -> Result<u32> {
        self.storage.set_name(link, name)
    }

    pub fn get_name_by_external_reference(&self, link: u32) -> Result<Option<String>> {
        self.storage.get_name_by_external_reference(link)
    }

    pub fn get_name(&self, link: u32) -> Result<Option<String>> {
        self.storage.get_name(link)
    }

    pub fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        self.storage.get_by_name(name)
    }

    pub fn get_external_reference_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        self.storage.get_external_reference_by_name(name)
    }

    pub fn remove_name(&mut self, link: u32) -> Result<()> {
        self.storage.remove_name(link)
    }

    pub fn remove_name_by_external_reference(&mut self, external_reference_id: u32) -> Result<()> {
        self.storage
            .remove_name_by_external_reference(external_reference_id)
    }

    pub fn unicode_storage(&self) -> &UnicodeStringStorage<'a> {
        &self.storage
    }

    pub fn unicode_storage_mut(&mut self) -> &mut UnicodeStringStorage<'a> {
        &mut self.storage
    }
}
