use crate::link_storage::LinkStorage;
use crate::sequences::AddressToRawNumberConverter;

#[derive(Clone, Copy, Debug)]
pub struct CharToUnicodeSymbolConverter {
    address_to_number_converter: AddressToRawNumberConverter,
    unicode_symbol_type: u32,
}

impl CharToUnicodeSymbolConverter {
    pub fn new(
        address_to_number_converter: AddressToRawNumberConverter,
        unicode_symbol_type: u32,
    ) -> Self {
        Self {
            address_to_number_converter,
            unicode_symbol_type,
        }
    }

    pub fn convert(&self, links: &mut LinkStorage, code_unit: u16) -> u32 {
        let raw_number = self
            .address_to_number_converter
            .convert(u32::from(code_unit));
        links.get_or_create(raw_number, self.unicode_symbol_type)
    }
}
