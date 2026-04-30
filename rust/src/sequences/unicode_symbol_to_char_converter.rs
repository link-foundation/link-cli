use anyhow::{bail, Result};

use crate::link_storage::LinkStorage;
use crate::sequences::{RawNumberToAddressConverter, TargetMatcher};

#[derive(Clone, Copy, Debug)]
pub struct UnicodeSymbolToCharConverter {
    number_to_address_converter: RawNumberToAddressConverter,
    unicode_symbol_criterion_matcher: TargetMatcher,
}

impl UnicodeSymbolToCharConverter {
    pub fn new(
        number_to_address_converter: RawNumberToAddressConverter,
        unicode_symbol_criterion_matcher: TargetMatcher,
    ) -> Self {
        Self {
            number_to_address_converter,
            unicode_symbol_criterion_matcher,
        }
    }

    pub fn convert(&self, links: &LinkStorage, symbol: u32) -> Result<u16> {
        if !self
            .unicode_symbol_criterion_matcher
            .is_matched(links, symbol)
        {
            bail!("Specified link {symbol} is not a Unicode symbol.");
        }

        let Some(link) = links.get(symbol) else {
            bail!("Unicode symbol link {symbol} does not exist.");
        };
        let code_unit = self.number_to_address_converter.convert(link.source);
        Ok(u16::try_from(code_unit)?)
    }
}
