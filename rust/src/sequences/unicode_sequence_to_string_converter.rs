use anyhow::{bail, Result};

use crate::link_storage::LinkStorage;
use crate::sequences::{RightSequenceWalker, TargetMatcher, UnicodeSymbolToCharConverter};

#[derive(Clone, Copy, Debug)]
pub struct UnicodeSequenceToStringConverter {
    unicode_sequence_criterion_matcher: TargetMatcher,
    sequence_walker: RightSequenceWalker,
    unicode_symbol_to_char_converter: UnicodeSymbolToCharConverter,
    unicode_sequence_type: u32,
}

impl UnicodeSequenceToStringConverter {
    pub fn new(
        unicode_sequence_criterion_matcher: TargetMatcher,
        sequence_walker: RightSequenceWalker,
        unicode_symbol_to_char_converter: UnicodeSymbolToCharConverter,
        unicode_sequence_type: u32,
    ) -> Self {
        Self {
            unicode_sequence_criterion_matcher,
            sequence_walker,
            unicode_symbol_to_char_converter,
            unicode_sequence_type,
        }
    }

    pub fn convert(&self, links: &LinkStorage, sequence: u32) -> Result<String> {
        if sequence == self.unicode_sequence_type {
            return Ok(String::new());
        }
        if !self
            .unicode_sequence_criterion_matcher
            .is_matched(links, sequence)
        {
            bail!("Specified link {sequence} is not a Unicode sequence.");
        }

        let Some(sequence_link) = links.get(sequence) else {
            bail!("Unicode sequence link {sequence} does not exist.");
        };
        let code_units = self
            .sequence_walker
            .walk(links, sequence_link.source)
            .into_iter()
            .map(|symbol| self.unicode_symbol_to_char_converter.convert(links, symbol))
            .collect::<Result<Vec<_>>>()?;

        Ok(String::from_utf16(&code_units)?)
    }
}
