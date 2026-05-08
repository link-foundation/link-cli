//! Unicode string and name storage backed by doublet links.
//!
//! This mirrors the C# `UnicodeStringStorage<uint>` constructor pipeline:
//! pinned types, `BalancedVariantConverter`, target matchers, Unicode symbol
//! converters, string/sequence converters, right-sequence walking, and
//! `NamedLinks`.

use std::cell::RefCell;

use anyhow::{bail, Result};

use crate::hybrid_reference::{external_reference, external_reference_value};
use crate::link_storage::LinkStorage;
use crate::named_links::NamedLinks;
use crate::pinned_types::PinnedTypes;
use crate::sequences::{
    AddressToRawNumberConverter, BalancedVariantConverter, CachingConverterDecorator,
    CharToUnicodeSymbolConverter, RawNumberToAddressConverter, RightSequenceWalker,
    StringToUnicodeSequenceConverter, TargetMatcher, UnicodeSequenceToStringConverter,
    UnicodeSymbolToCharConverter,
};

/// Link-backed Unicode string storage with C# pinned type layout.
pub struct UnicodeStringStorage<'a> {
    links: &'a mut LinkStorage,
    type_type: u32,
    unicode_symbol_type: u32,
    unicode_sequence_type: u32,
    string_type: u32,
    empty_string_type: u32,
    name_type: u32,
    address_to_number_converter: AddressToRawNumberConverter,
    number_to_address_converter: RawNumberToAddressConverter,
    balanced_variant_converter: BalancedVariantConverter,
    unicode_symbol_criterion_matcher: TargetMatcher,
    unicode_sequence_criterion_matcher: TargetMatcher,
    char_to_unicode_symbol_converter: CharToUnicodeSymbolConverter,
    unicode_symbol_to_char_converter: UnicodeSymbolToCharConverter,
    string_to_unicode_sequence_converter: StringToUnicodeSequenceConverter,
    sequence_walker: RightSequenceWalker,
    unicode_sequence_to_string_converter: UnicodeSequenceToStringConverter,
    string_to_unicode_sequence_cache: CachingConverterDecorator<String, u32>,
    unicode_sequence_to_string_cache: RefCell<CachingConverterDecorator<u32, String>>,
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

        let address_to_number_converter = AddressToRawNumberConverter::new();
        let number_to_address_converter = RawNumberToAddressConverter::new();
        let balanced_variant_converter = BalancedVariantConverter::new();
        let unicode_symbol_criterion_matcher = TargetMatcher::new(unicode_symbol_type);
        let unicode_sequence_criterion_matcher = TargetMatcher::new(unicode_sequence_type);
        let char_to_unicode_symbol_converter =
            CharToUnicodeSymbolConverter::new(address_to_number_converter, unicode_symbol_type);
        let unicode_symbol_to_char_converter = UnicodeSymbolToCharConverter::new(
            number_to_address_converter,
            unicode_symbol_criterion_matcher,
        );
        let string_to_unicode_sequence_converter = StringToUnicodeSequenceConverter::new(
            char_to_unicode_symbol_converter,
            balanced_variant_converter,
            unicode_sequence_type,
        );
        let sequence_walker = RightSequenceWalker::new(unicode_symbol_criterion_matcher);
        let unicode_sequence_to_string_converter = UnicodeSequenceToStringConverter::new(
            unicode_sequence_criterion_matcher,
            sequence_walker,
            unicode_symbol_to_char_converter,
            unicode_sequence_type,
        );

        let mut storage = Self {
            links,
            type_type,
            unicode_symbol_type,
            unicode_sequence_type,
            string_type,
            empty_string_type,
            name_type,
            address_to_number_converter,
            number_to_address_converter,
            balanced_variant_converter,
            unicode_symbol_criterion_matcher,
            unicode_sequence_criterion_matcher,
            char_to_unicode_symbol_converter,
            unicode_symbol_to_char_converter,
            string_to_unicode_sequence_converter,
            sequence_walker,
            unicode_sequence_to_string_converter,
            string_to_unicode_sequence_cache: CachingConverterDecorator::new(),
            unicode_sequence_to_string_cache: RefCell::new(CachingConverterDecorator::new()),
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
        NamedLinks::from_storage(self)
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

    pub fn address_to_number_converter(&self) -> AddressToRawNumberConverter {
        self.address_to_number_converter
    }

    pub fn number_to_address_converter(&self) -> RawNumberToAddressConverter {
        self.number_to_address_converter
    }

    pub fn balanced_variant_converter(&self) -> BalancedVariantConverter {
        self.balanced_variant_converter
    }

    pub fn unicode_symbol_criterion_matcher(&self) -> TargetMatcher {
        self.unicode_symbol_criterion_matcher
    }

    pub fn unicode_sequence_criterion_matcher(&self) -> TargetMatcher {
        self.unicode_sequence_criterion_matcher
    }

    pub fn char_to_unicode_symbol_converter(&self) -> CharToUnicodeSymbolConverter {
        self.char_to_unicode_symbol_converter
    }

    pub fn unicode_symbol_to_char_converter(&self) -> UnicodeSymbolToCharConverter {
        self.unicode_symbol_to_char_converter
    }

    pub fn string_to_unicode_sequence_converter(&self) -> StringToUnicodeSequenceConverter {
        self.string_to_unicode_sequence_converter
    }

    pub fn sequence_walker(&self) -> RightSequenceWalker {
        self.sequence_walker
    }

    pub fn unicode_sequence_to_string_converter(&self) -> UnicodeSequenceToStringConverter {
        self.unicode_sequence_to_string_converter
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
        if !self
            .unicode_sequence_criterion_matcher
            .is_matched(self.links, sequence)
        {
            bail!("Link {sequence} is not a Unicode sequence.");
        }
        let unicode_sequence = self
            .links
            .get(sequence)
            .ok_or_else(|| anyhow::anyhow!("Unicode sequence link {sequence} does not exist."))?;

        self.sequence_walker
            .walk(self.links, unicode_sequence.source)
            .into_iter()
            .map(|symbol| {
                self.unicode_symbol_to_char_converter
                    .convert(self.links, symbol)
            })
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
            let Some(candidate) = self.links.get(name_candidate) else {
                continue;
            };
            if candidate.source == self.name_type {
                return self.get_string(candidate.target).map(Some);
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
        let input = content.to_string();
        if let Some(cached) = self.string_to_unicode_sequence_cache.get(&input) {
            return cached;
        }

        let converter = self.string_to_unicode_sequence_converter;
        let sequence = converter.convert(self.links, content);
        self.string_to_unicode_sequence_cache
            .insert(input, sequence)
    }

    fn unicode_sequence_to_string(&self, sequence: u32) -> Result<String> {
        if let Some(cached) = self
            .unicode_sequence_to_string_cache
            .borrow()
            .get(&sequence)
        {
            return Ok(cached);
        }

        let output = self
            .unicode_sequence_to_string_converter
            .convert(self.links, sequence)?;
        self.unicode_sequence_to_string_cache
            .borrow_mut()
            .insert(sequence, output.clone());
        Ok(output)
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
